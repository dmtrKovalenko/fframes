#![allow(clippy::too_many_arguments)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

#[inline(always)]
pub fn get_rgb(pixmap: &[u8], i: usize) -> (i32, i32, i32) {
    let r = pixmap[4 * i] as i32;
    let g = pixmap[4 * i + 1] as i32;
    let b = pixmap[4 * i + 2] as i32;

    (r, g, b)
}

/// ! Publicly exported only for benchmarking.
/// We support only yuv420 format as for now so we can pretty efficiently convert the bitmap buffer.
/// yuv420 represented by y per each pixel and uv (cb and cr) per each 2x2 pixel block.
#[allow(clippy::precedence)]
pub fn fill_yuv420_from_rgba_pixmap_base(
    width: i32,
    height: i32,
    y_linesize: i32,
    cb_linesize: i32,
    cr_linesize: i32,
    rgba_pixels: &[u8],
    y_pixels_destination: *mut u8,
    cb_pixels_destination: *mut u8,
    cr_pixels_destination: *mut u8,
) {
    unsafe {
        // an important note that linesize here can be different from the width of an image so it is required to fill the buffer correctly.
        let width = width as usize;
        let height = height as usize;
        let frame_size = height * (y_linesize as usize) + width;

        let y_pixels = std::slice::from_raw_parts_mut(y_pixels_destination, frame_size);
        let cb_pixels = std::slice::from_raw_parts_mut(cb_pixels_destination, frame_size / 2);
        let cr_pixels = std::slice::from_raw_parts_mut(cr_pixels_destination, frame_size / 2);

        for y in 0..height {
            for x in 0..width {
                let (r, g, b) = get_rgb(rgba_pixels, y * width + x);

                // use a linesize to get the correct index for the pixel as it can differ for different dimensions.
                y_pixels[y * y_linesize as usize + x] =
                    (16 + (66 * r + 129 * g + 25 * b) >> 8) as u8;

                if y % 2 == 0 && x % 2 == 0 {
                    // the bounds are 1/4 of the image size
                    let x = x / 2;
                    let y = y / 2;

                    cb_pixels[y * cb_linesize as usize + x] =
                        (128 + ((-38 * r) - (74 * g) + (112 * b) >> 8)) as u8;
                    cr_pixels[y * cr_linesize as usize + x] =
                        (128 + ((112 * r) - (94 * g) - (18 * b) >> 8)) as u8;
                }
            }
        }
    }
}

/// ! Publicly exported only for benchmarking.
/// Accelerated version of the yuv420 for neon using SIMD instructions.
#[cfg(target_feature = "neon")]
pub unsafe fn fill_yuv420_from_rgba_pixmap_accelerated(
    width: i32,
    height: i32,
    y_linesize: i32,
    cb_linesize: i32,
    cr_linesize: i32,
    rgba_pixels: &[u8],
    y_pixels_destination: *mut u8,
    cb_pixels_destination: *mut u8,
    cr_pixels_destination: *mut u8,
) {
    // the asm implementation rely on the fact that the resolution is dividable by v8
    // which is true for the most common resolution
    if width % 8 != 0 {
        return fill_yuv420_from_rgba_pixmap_base(
            width,
            height,
            y_linesize,
            cb_linesize,
            cr_linesize,
            rgba_pixels,
            y_pixels_destination,
            cb_pixels_destination,
            cr_pixels_destination,
        );
    }

    unsafe {
        std::arch::asm!(
            // setup conversion coefficients
            "movi v20.8h, #66",         // r coef for y
            "movi v21.8h, #129",        // g coef for y
            "movi v22.8h, #25",         // b coef for y

            "movi v23.8h, #38",         // setup cb coeffs
            "neg v23.8h, v23.8h",       // -38 r (this is correct as negative)
            "movi v24.8h, #74",         // 74 g (positive)
            "movi v25.8h, #112",        // 112 b (positive)

            "movi v26.8h, #112",        // setup cr coeffs (r is positive)
            "movi v27.8h, #94",         // 94 g (positive)
            "movi v28.8h, #18",         // 18 b (positive)

            // constants
            "movi v29.8h, #16",         // y offset
            "movi v30.8h, #128",        // cb/cr offset

            "mov w9, wzr",              // y = 0

            "2:",                       // row loop
                "add x1, {src}, {width:x}, lsl #2", // Next row start (width * 4 bytes per pixel)
                "prfm pldl1keep, [x1]",             // Prefetch next row data

                "mov w10, wzr",         // x = 0
                "3:",                   // col loop (8 pixels)
                    // load 8 rgba pixels
                    "ld4 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [{src}], #32",

                    // convert rgb to 16-bit
                    "uxtl v4.8h, v0.8b", // r
                    "uxtl v6.8h, v1.8b", // g
                    "uxtl v8.8h, v2.8b", // b

                    // calc y
                    "mul.8h v10, v4, v20",   // r * 66
                    "mla.8h v10, v6, v21",   // + g * 129
                    "mla.8h v10, v8, v22",   // + b * 25
                    "addhn.8b v12, v10, v29",  // + 16 offset and pack
                    // store y
                    "st1 {{v12.8b}}, [{dst_y}], #8",

                    // only process cb/cr on even rows
                    "tbnz w9, #0, 5f",

                    // Extract even-indexed pixels (0, 2, 4, 6)
                    "uzp1 v13.8h, v4.8h, v4.8h",
                    "uzp1 v14.8h, v6.8h, v6.8h",
                    "uzp1 v15.8h, v8.8h, v8.8h",

                    // calc cb for even pixels: 128 + ((-38*R - 74*G + 112*B) >> 8)
                    "mul.8h v16, v13, v23",  // r * -38
                    "mls.8h v16, v14, v24",  // - g * 74
                    "mla.8h v16, v15, v25",  // + b * 112
                    "sshr.8h v16, v16, #8",     // >> 8
                    "add.8h v16, v16, v30",  // add 128 offset
                    "sqxtun.8b v17, v16",       // convert to unsigned

                    // calc cr for even pixels: 128 + ((112*R - 94*G - 18*B) >> 8)
                    "mul.8h v18, v13, v26",  // r * 112
                    "mls.8h v18, v14, v27",  // - g * 94 (subtract using mls)
                    "mls.8h v18, v15, v28",  // - b * 18 (subtract using mls)
                    "sshr.8h v18, v18, #8",     // >> 8
                    "add.8h v18, v18, v30",  // add 128 offset
                    "sqxtun.8b v19, v18",       // convert to unsigned

                    // Store 4 bytes
                    "str s17, [{dst_cb}], #4",
                    "str s19, [{dst_cr}], #4",

                    "5:",
                    "add w10, w10, #8",            // go to next 8 pixels
                    "cmp w10, {width:w}",
                    "b.lt 3b",

                // end of row
                "add {dst_y}, {dst_y}, {y_pad:x}",

                // only update padding on even rows
                "tbnz w9, #0, 7f",

                "add {dst_cb}, {dst_cb}, {cb_pad:x}",
                "add {dst_cr}, {dst_cr}, {cr_pad:x}",
                "7:",

                "add w9, w9, #1",                  // next row
                "cmp w9, {height:w}",
                "b.lt 2b",

            src = in(reg) rgba_pixels.as_ptr(),
            dst_y = in(reg) y_pixels_destination,
            dst_cb = in(reg) cb_pixels_destination,
            dst_cr = in(reg) cr_pixels_destination,
            width = in(reg) width,
            height = in(reg) height,
            y_pad = in(reg) (y_linesize - width),
            cb_pad = in(reg) (cb_linesize - (width / 2)),
            cr_pad = in(reg) (cr_linesize - (width / 2)),

            out("x1") _, out("w4") _, out("x5") _, out("w6") _, out("w7") _, out("x8") _,
            out("w9") _, out("w10") _, out("w11") _, out("w12") _,
            out("v0") _, out("v1") _, out("v2") _, out("v3") _, out("v4") _,
            out("v6") _, out("v8") _, out("v10") _, out("v12") _, out("v13") _,
            out("v14") _, out("v15") _, out("v16") _, out("v17") _, out("v18") _,
            out("v19") _, out("v20") _, out("v21") _, out("v22") _, out("v23") _,
            out("v24") _, out("v25") _, out("v26") _, out("v27") _, out("v28") _,
            out("v29") _, out("v30") _,

            options(nostack)
        );
    }
}

#[cfg(not(target_feature = "neon"))]
pub unsafe fn fill_yuv420_from_rgba_pixmap_accelerated(
    width: i32,
    height: i32,
    y_linesize: i32,
    cb_linesize: i32,
    cr_linesize: i32,
    rgba_pixels: &[u8],
    y_pixels_destination: *mut u8,
    cb_pixels_destination: *mut u8,
    cr_pixels_destination: *mut u8,
) {
    fill_yuv420_from_rgba_pixmap_base(
        width,
        height,
        y_linesize,
        cb_linesize,
        cr_linesize,
        rgba_pixels,
        y_pixels_destination,
        cb_pixels_destination,
        cr_pixels_destination,
    );
}
