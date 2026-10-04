#![allow(clippy::too_many_arguments)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

/// The RGB to `YCbCr` conversion every encoder input goes through (BT.601, limited range),
/// as weights for channel values in `0.0..=1.0`: `[r, g, b, offset]` for Y, Cb and Cr.
///
/// These are the coefficients of the integer converter below. Backends that convert on the
/// GPU use them too, so the streams (tagged as BT.601 limited by the encoder) look the same
/// whichever way a frame took.
pub const RGB_TO_YUV: [[f32; 4]; 3] = [
    [66. / 256., 129. / 256., 25. / 256., 16. / 255.],
    [-38. / 256., -74. / 256., 112. / 256., 128. / 255.],
    [112. / 256., -94. / 256., -18. / 256., 128. / 255.],
];

#[inline(always)]
pub fn get_rgb(pixmap: &[u8], i: usize) -> (i32, i32, i32) {
    let r = i32::from(pixmap[4 * i]);
    let g = i32::from(pixmap[4 * i + 1]);
    let b = i32::from(pixmap[4 * i + 2]);

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
        if width == 0 || height == 0 {
            return;
        }

        // the last line of a plane ends with its last pixel, not with the line padding
        let plane_len =
            |linesize: i32, columns: usize, rows: usize| (rows - 1) * linesize as usize + columns;
        let (chroma_width, chroma_height) = (width.div_ceil(2), height.div_ceil(2));

        let y_pixels = std::slice::from_raw_parts_mut(
            y_pixels_destination,
            plane_len(y_linesize, width, height),
        );
        let cb_pixels = std::slice::from_raw_parts_mut(
            cb_pixels_destination,
            plane_len(cb_linesize, chroma_width, chroma_height),
        );
        let cr_pixels = std::slice::from_raw_parts_mut(
            cr_pixels_destination,
            plane_len(cr_linesize, chroma_width, chroma_height),
        );

        for y in 0..height {
            for x in 0..width {
                let (r, g, b) = get_rgb(rgba_pixels, y * width + x);

                // use a linesize to get the correct index for the pixel as it can differ for different dimensions.
                // BT.601 limited range: black is 16, white is 235 (+128 rounds the >> 8).
                y_pixels[y * y_linesize as usize + x] =
                    (16 + ((66 * r + 129 * g + 25 * b + 128) >> 8)) as u8;

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
    if width <= 0 || height <= 0 {
        return;
    }
    if width < 8 {
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

    // Keep full eight-pixel blocks on NEON even when a row has a scalar tail.
    let vector_width = width & !7;
    unsafe {
        std::arch::asm!(
            // setup conversion coefficients
            "movi v20.8b, #66",         // r coef for y
            "movi v21.8b, #129",        // g coef for y
            "movi v22.8b, #25",         // b coef for y

            "movi v23.8b, #38",         // r coef for cb
            "movi v24.8b, #74",         // 74 g (positive)
            "movi v25.8b, #112",        // 112 b (positive)

            "movi v26.8b, #112",        // setup cr coeffs (r is positive)
            "movi v27.8b, #94",         // 94 g (positive)
            "movi v28.8b, #18",         // 18 b (positive)

            // constants
            // y offset: (16 << 8) + 128, so the high-half narrow below yields
            // 16 + round(sum / 256) (black = 16, white = 235)
            "movi v29.8h, #16, lsl #8",
            "orr v29.8h, #128",
            "movi v30.8h, #128, lsl #8",        // cb/cr offset before narrowing

            "mov w9, wzr",              // y = 0

            "2:",                       // row loop
                "add x1, {src}, {width:x}, lsl #2", // Next row start (width * 4 bytes per pixel)
                "prfm pldl1keep, [x1]",             // Prefetch next row data

                "mov w10, wzr",         // x = 0
                "3:",                   // col loop (8 pixels)
                    // load 8 rgba pixels
                    "ld4 {{v0.8b, v1.8b, v2.8b, v3.8b}}, [{src}], #32",

                    // Widen while multiplying instead of widening each channel first.
                    "umull v10.8h, v0.8b, v20.8b",
                    "umlal v10.8h, v1.8b, v21.8b",
                    "umlal v10.8h, v2.8b, v22.8b",
                    "addhn v12.8b, v10.8h, v29.8h",
                    // store y
                    "st1 {{v12.8b}}, [{dst_y}], #8",

                    // only process cb/cr on even rows
                    "tbnz w9, #0, 5f",

                    // Only the four even pixels contribute chroma.
                    "uzp1 v13.8b, v0.8b, v0.8b",
                    "uzp1 v14.8b, v1.8b, v1.8b",
                    "uzp1 v15.8b, v2.8b, v2.8b",

                    // Chroma sums fit signed 16 bits. Adding 128 << 8 before
                    // narrowing gives the same floor division as the scalar converter.
                    "umull v16.8h, v15.8b, v25.8b",
                    "umlsl v16.8h, v13.8b, v23.8b",
                    "umlsl v16.8h, v14.8b, v24.8b",
                    "addhn v17.8b, v16.8h, v30.8h",

                    "umull v18.8h, v13.8b, v26.8b",
                    "umlsl v18.8h, v14.8b, v27.8b",
                    "umlsl v18.8h, v15.8b, v28.8b",
                    "addhn v19.8b, v18.8h, v30.8h",

                    // Store 4 bytes
                    "str s17, [{dst_cb}], #4",
                    "str s19, [{dst_cr}], #4",

                    "5:",
                    "add w10, w10, #8",            // go to next 8 pixels
                    "cmp w10, {width:w}",
                    "b.lt 3b",

                // end of row
                "add {src}, {src}, {src_pad:x}",
                "add {dst_y}, {dst_y}, {y_pad:x}",

                // only update padding on even rows
                "tbnz w9, #0, 7f",

                "add {dst_cb}, {dst_cb}, {cb_pad:x}",
                "add {dst_cr}, {dst_cr}, {cr_pad:x}",
                "7:",

                "add w9, w9, #1",                  // next row
                "cmp w9, {height:w}",
                "b.lt 2b",

            // the pointers are advanced by the loop, and every value used as a 64 bit
            // register has to be passed as one (the upper half of an i32 is undefined)
            src = inout(reg) rgba_pixels.as_ptr() => _,
            dst_y = inout(reg) y_pixels_destination => _,
            dst_cb = inout(reg) cb_pixels_destination => _,
            dst_cr = inout(reg) cr_pixels_destination => _,
            width = in(reg) i64::from(vector_width),
            src_pad = in(reg) i64::from((width - vector_width) * 4),
            height = in(reg) i64::from(height),
            y_pad = in(reg) i64::from(y_linesize - vector_width),
            cb_pad = in(reg) i64::from(cb_linesize - (vector_width / 2)),
            cr_pad = in(reg) i64::from(cr_linesize - (vector_width / 2)),

            out("x1") _, out("w9") _, out("w10") _,
            out("v0") _, out("v1") _, out("v2") _, out("v3") _,
            out("v10") _, out("v12") _, out("v13") _,
            out("v14") _, out("v15") _, out("v16") _, out("v17") _, out("v18") _,
            out("v19") _, out("v20") _, out("v21") _, out("v22") _, out("v23") _,
            out("v24") _, out("v25") _, out("v26") _, out("v27") _, out("v28") _,
            out("v29") _, out("v30") _,

            options(nostack)
        );
        if vector_width != width {
            for row in 0..height as usize {
                for x in vector_width as usize..width as usize {
                    let (r, g, b) = get_rgb(rgba_pixels, row * width as usize + x);
                    *y_pixels_destination.add(row * y_linesize as usize + x) =
                        (16 + ((66 * r + 129 * g + 25 * b + 128) >> 8)) as u8;
                    if row % 2 == 0 && x % 2 == 0 {
                        *cb_pixels_destination.add((row / 2) * cb_linesize as usize + x / 2) =
                            (128 + ((-38 * r - 74 * g + 112 * b) >> 8)) as u8;
                        *cr_pixels_destination.add((row / 2) * cr_linesize as usize + x / 2) =
                            (128 + ((112 * r - 94 * g - 18 * b) >> 8)) as u8;
                    }
                }
            }
        }
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
