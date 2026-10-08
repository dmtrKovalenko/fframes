#![allow(clippy::too_many_arguments)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

/// The matrix every RGB to `YCbCr` conversion of the encoder input uses, limited range
/// either way. The encoder tags the stream with it, so players decode the colors that were
/// rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum YuvMatrix {
    /// ITU-R BT.601, tagged `smpte170m`.
    #[default]
    Bt601,
    /// ITU-R BT.709, tagged `bt709` for the matrix, the primaries and the transfer. The
    /// usual matrix of HD video, and what players assume for an untagged HD stream.
    Bt709,
}

impl YuvMatrix {
    /// The integer weights of the converters for 8 bit channels: `[r, g, b]` for Y, Cb and
    /// Cr, scaled by 256. Every row of Cb and Cr sums to 0, so grays get neutral chroma,
    /// and Y's sums to 220, so white is 235.
    pub const fn weights(self) -> [[i32; 3]; 3] {
        match self {
            Self::Bt601 => [[66, 129, 25], [-38, -74, 112], [112, -94, -18]],
            Self::Bt709 => [[47, 157, 16], [-26, -86, 112], [112, -102, -10]],
        }
    }

    /// The same conversion as weights for channel values in `0.0..=1.0`: `[r, g, b, offset]`
    /// for Y, Cb and Cr. Backends that convert on the GPU use these, so a stream looks the
    /// same whichever way a frame took.
    pub fn rgb_to_yuv(self) -> [[f32; 4]; 3] {
        let offsets = [16. / 255., 128. / 255., 128. / 255.];
        let weights = self.weights();
        std::array::from_fn(|row| {
            let [r, g, b] = weights[row].map(|weight| weight as f32 / 256.);
            [r, g, b, offsets[row]]
        })
    }
}

/// [`YuvMatrix::Bt601`] as [`YuvMatrix::rgb_to_yuv`] gives it.
pub const RGB_TO_YUV: [[f32; 4]; 3] = [
    [66. / 256., 129. / 256., 25. / 256., 16. / 255.],
    [-38. / 256., -74. / 256., 112. / 256., 128. / 255.],
    [112. / 256., -94. / 256., -18. / 256., 128. / 255.],
];

/// Y of one pixel with integer `weights` (see [`YuvMatrix::weights`]). Limited range:
/// black is 16, white is 235 (+128 rounds the >> 8).
#[inline(always)]
fn luma(weights: &[[i32; 3]; 3], (r, g, b): (i32, i32, i32)) -> u8 {
    let [y, _, _] = weights;
    (16 + ((y[0] * r + y[1] * g + y[2] * b + 128) >> 8)) as u8
}

/// Cb and Cr of one pixel with integer `weights`. The sums are floored, as the SIMD
/// converter does.
#[inline(always)]
fn chroma(weights: &[[i32; 3]; 3], (r, g, b): (i32, i32, i32)) -> (u8, u8) {
    let [_, cb, cr] = weights;
    (
        (128 + ((cb[0] * r + cb[1] * g + cb[2] * b) >> 8)) as u8,
        (128 + ((cr[0] * r + cr[1] * g + cr[2] * b) >> 8)) as u8,
    )
}

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
pub fn fill_yuv420_from_rgba_pixmap_base(
    matrix: YuvMatrix,
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

        let weights = matrix.weights();
        for y in 0..height {
            for x in 0..width {
                // use a linesize to get the correct index for the pixel as it can differ for different dimensions.
                let rgb = get_rgb(rgba_pixels, y * width + x);
                y_pixels[y * y_linesize as usize + x] = luma(&weights, rgb);

                if y % 2 == 0 && x % 2 == 0 {
                    // the bounds are 1/4 of the image size
                    let x = x / 2;
                    let y = y / 2;

                    let (cb, cr) = chroma(&weights, rgb);
                    cb_pixels[y * cb_linesize as usize + x] = cb;
                    cr_pixels[y * cr_linesize as usize + x] = cr;
                }
            }
        }
    }
}

/// ! Publicly exported only for benchmarking.
/// Accelerated version of the yuv420 for neon using SIMD instructions.
#[cfg(target_feature = "neon")]
pub unsafe fn fill_yuv420_from_rgba_pixmap_accelerated(
    matrix: YuvMatrix,
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
            matrix,
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
    let weights = matrix.weights();
    // The SIMD code multiplies unsigned bytes and subtracts the negative weights.
    let mut coefficients = [0_u8; 16];
    for (byte, weight) in coefficients.iter_mut().zip(weights.as_flattened()) {
        *byte = weight.unsigned_abs() as u8;
    }
    unsafe {
        std::arch::asm!(
            // setup conversion coefficients, the magnitudes of `matrix.weights()`:
            // Y's r, g, b, then Cb's r, g (subtracted) and b, then Cr's r, g and b
            // (both subtracted)
            "ld1 {{v31.16b}}, [{coefficients}]",
            "dup v20.8b, v31.b[0]",
            "dup v21.8b, v31.b[1]",
            "dup v22.8b, v31.b[2]",

            "dup v23.8b, v31.b[3]",
            "dup v24.8b, v31.b[4]",
            "dup v25.8b, v31.b[5]",

            "dup v26.8b, v31.b[6]",
            "dup v27.8b, v31.b[7]",
            "dup v28.8b, v31.b[8]",

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
            coefficients = in(reg) coefficients.as_ptr(),
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
            out("v29") _, out("v30") _, out("v31") _,

            options(nostack)
        );
        if vector_width != width {
            for row in 0..height as usize {
                for x in vector_width as usize..width as usize {
                    let rgb = get_rgb(rgba_pixels, row * width as usize + x);
                    *y_pixels_destination.add(row * y_linesize as usize + x) = luma(&weights, rgb);
                    if row % 2 == 0 && x % 2 == 0 {
                        let (cb, cr) = chroma(&weights, rgb);
                        *cb_pixels_destination.add((row / 2) * cb_linesize as usize + x / 2) = cb;
                        *cr_pixels_destination.add((row / 2) * cr_linesize as usize + x / 2) = cr;
                    }
                }
            }
        }
    }
}

#[cfg(not(target_feature = "neon"))]
pub unsafe fn fill_yuv420_from_rgba_pixmap_accelerated(
    matrix: YuvMatrix,
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
        matrix,
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
