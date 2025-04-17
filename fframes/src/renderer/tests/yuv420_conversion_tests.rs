use crate::renderer::pix_fmt::fill_yuv420_from_rgba_pixmap_base;

struct TestImage {
    width: i32,
    height: i32,
    rgba_pixels: Vec<u8>,
    y_pixels: Vec<u8>,
    cb_pixels: Vec<u8>,
    cr_pixels: Vec<u8>,
}

impl TestImage {
    fn new(width: i32, height: i32) -> Self {
        let frame_size = (width * height) as usize;
        Self {
            width,
            height,
            rgba_pixels: vec![0; frame_size * 4], // RGBA = 4 bytes per pixel
            y_pixels: vec![0; frame_size],
            cb_pixels: vec![0; frame_size / 4],
            cr_pixels: vec![0; frame_size / 4],
        }
    }

    fn with_solid_color(width: i32, height: i32, r: u8, g: u8, b: u8, a: u8) -> Self {
        let mut img = Self::new(width, height);
        for chunk in img.rgba_pixels.chunks_exact_mut(4) {
            chunk[0] = r;
            chunk[1] = g;
            chunk[2] = b;
            chunk[3] = a;
        }
        img
    }

    fn random(width: i32, height: i32) -> Self {
        let mut img = Self::new(width, height);
        for chunk in img.rgba_pixels.chunks_exact_mut(4) {
            chunk[0] = rand::random();
            chunk[1] = rand::random();
            chunk[2] = rand::random();
            chunk[3] = 255; // Alpha channel
        }
        img
    }

    fn create_test_patterns() -> Vec<Self> {
        vec![
            Self::with_solid_color(8, 8, 255, 0, 0, 255), // Pure red
            Self::with_solid_color(8, 24, 255, 0, 0, 255), // Pure red
            Self::with_solid_color(16, 16, 0, 255, 0, 255), // Pure green
            Self::with_solid_color(16, 16, 0, 0, 255, 255), // Pure blue
            Self::with_solid_color(16, 16, 255, 255, 255, 255), // White
            Self::with_solid_color(16, 16, 0, 0, 0, 255), // Black
            Self::with_solid_color(32, 24, 128, 128, 128, 255), // Gray
            Self::random(32, 24),
            Self::random(64, 48),
        ]
    }
}

struct YuvVerifyInput<'a> {
    rgba: &'a [u8],
    y: &'a [u8],
    cb: &'a [u8],
    cr: &'a [u8],
    width: usize,
    height: usize,
    y_linesize: usize,
    cb_linesize: usize,
    cr_linesize: usize,
}

fn verify_yuv_values(input: YuvVerifyInput) {
    for y_pos in 0..input.height {
        for x_pos in 0..input.width {
            let rgba_idx = (y_pos * input.width + x_pos) * 4;
            let y_idx = y_pos * input.y_linesize + x_pos;

            let r = input.rgba[rgba_idx];
            let g = input.rgba[rgba_idx + 1];
            let b = input.rgba[rgba_idx + 2];

            // Verify Y value using the same formula as in the implementation
            let expected_y =
                ((16_i32 + (66 * r as i32 + 129 * g as i32 + 25 * b as i32)) >> 8) as u8;
            assert_eq!(
                input.y[y_idx], expected_y,
                "Y mismatch at ({}, {}): expected {}, got {}",
                x_pos, y_pos, expected_y, input.y[y_idx]
            );

            // Check CB and CR only for even coordinates
            if y_pos % 2 == 0 && x_pos % 2 == 0 {
                let cb_idx = (y_pos / 2) * input.cb_linesize + (x_pos / 2);
                let cr_idx = (y_pos / 2) * input.cr_linesize + (x_pos / 2);

                let expected_cb = (128_i32
                    + (((-38 * r as i32) - (74 * g as i32) + (112 * b as i32)) >> 8))
                    as u8;
                let expected_cr =
                    (128_i32 + (((112 * r as i32) - (94 * g as i32) - (18 * b as i32)) >> 8)) as u8;

                assert_eq!(
                    input.cb[cb_idx], expected_cb,
                    "Cb mismatch at ({}, {}): expected {}, got {}",
                    x_pos, y_pos, expected_cb, input.cb[cb_idx]
                );
                assert_eq!(
                    input.cr[cr_idx], expected_cr,
                    "Cr mismatch at ({}, {}): expected {}, got {}",
                    x_pos, y_pos, expected_cr, input.cr[cr_idx]
                );
            }
        }
    }
}

#[test]
fn yuv_base_implementation() {
    for test_image in TestImage::create_test_patterns() {
        let TestImage {
            width,
            height,
            rgba_pixels,
            mut y_pixels,
            mut cb_pixels,
            mut cr_pixels,
        } = test_image;

        fill_yuv420_from_rgba_pixmap_base(
            width,
            height,
            width,     // y_linesize same as width for simplicity
            width / 2, // cb_linesize
            width / 2, // cr_linesize
            &rgba_pixels,
            y_pixels.as_mut_ptr(),
            cb_pixels.as_mut_ptr(),
            cr_pixels.as_mut_ptr(),
        );

        verify_yuv_values(YuvVerifyInput {
            rgba: &rgba_pixels,
            y: &y_pixels,
            cb: &cb_pixels,
            cr: &cr_pixels,
            width: width as usize,
            height: height as usize,
            y_linesize: width as usize,
            cb_linesize: (width / 2) as usize,
            cr_linesize: (width / 2) as usize,
        });
    }
}

#[test]
#[cfg(target_feature = "neon")]
fn yuv_neon_implementation() {
    use crate::pix_fmt::fill_yuv420_from_rgba_pixmap_accelerated;
    for test_image in TestImage::create_test_patterns() {
        let TestImage {
            width,
            height,
            rgba_pixels,
            mut y_pixels,
            mut cb_pixels,
            mut cr_pixels,
        } = test_image;

        unsafe {
            fill_yuv420_from_rgba_pixmap_accelerated(
                width,
                height,
                width,     // y_linesize same as width for simplicity
                width / 2, // cb_linesize
                width / 2, // cr_linesize
                &rgba_pixels,
                y_pixels.as_mut_ptr(),
                cb_pixels.as_mut_ptr(),
                cr_pixels.as_mut_ptr(),
            );
        }

        verify_yuv_values(YuvVerifyInput {
            rgba: &rgba_pixels,
            y: &y_pixels,
            cb: &cb_pixels,
            cr: &cr_pixels,
            width: width as usize,
            height: height as usize,
            y_linesize: width as usize,
            cb_linesize: (width / 2) as usize,
            cr_linesize: (width / 2) as usize,
        });
    }
}

#[test]
#[cfg(target_feature = "neon")]
fn yuv_neon_matches_base() {
    use crate::pix_fmt::fill_yuv420_from_rgba_pixmap_accelerated;
    for test_image in TestImage::create_test_patterns() {
        let TestImage {
            width,
            height,
            rgba_pixels,
            ..
        } = test_image;

        let mut y_pixels_base = vec![0u8; (width * height) as usize];
        let mut cb_pixels_base = vec![0u8; (width * height / 4) as usize];
        let mut cr_pixels_base = vec![0u8; (width * height / 4) as usize];

        let mut y_pixels_neon = y_pixels_base.clone();
        let mut cb_pixels_neon = cb_pixels_base.clone();
        let mut cr_pixels_neon = cr_pixels_base.clone();

        unsafe {
            // Run base implementation
            fill_yuv420_from_rgba_pixmap_base(
                width,
                height,
                width,
                width / 2,
                width / 2,
                &rgba_pixels,
                y_pixels_base.as_mut_ptr(),
                cb_pixels_base.as_mut_ptr(),
                cr_pixels_base.as_mut_ptr(),
            );

            // Run NEON implementation
            fill_yuv420_from_rgba_pixmap_accelerated(
                width,
                height,
                width,
                width / 2,
                width / 2,
                &rgba_pixels,
                y_pixels_neon.as_mut_ptr(),
                cb_pixels_neon.as_mut_ptr(),
                cr_pixels_neon.as_mut_ptr(),
            );
        }

        // Compare results
        assert_eq!(y_pixels_base, y_pixels_neon, "Y planes differ");
        assert_eq!(cb_pixels_base, cb_pixels_neon, "Cb planes differ");
        assert_eq!(cr_pixels_base, cr_pixels_neon, "Cr planes differ");
    }
}

#[test]
#[cfg(target_feature = "neon")]
fn yuv_neon_matches_base_with_padded_linesize() {
    use crate::pix_fmt::fill_yuv420_from_rgba_pixmap_accelerated;
    for test_image in TestImage::create_test_patterns() {
        let TestImage {
            width,
            height,
            rgba_pixels,
            ..
        } = test_image;

        // Create buffers with padding (linesize > width)
        let y_linesize = (width + 32) & !15; // Align to 16 bytes with some padding
        let uv_linesize = (width / 2 + 16) & !7; // Align to 8 bytes with some padding

        let y_buffer_size = y_linesize as usize * height as usize;
        let uv_buffer_size = uv_linesize as usize * (height as usize / 2);

        let mut y_pixels_base = vec![0u8; y_buffer_size];
        let mut cb_pixels_base = vec![0u8; uv_buffer_size];
        let mut cr_pixels_base = vec![0u8; uv_buffer_size];

        let mut y_pixels_neon = vec![0u8; y_buffer_size];
        let mut cb_pixels_neon = vec![0u8; uv_buffer_size];
        let mut cr_pixels_neon = vec![0u8; uv_buffer_size];

        unsafe {
            // Run base implementation
            fill_yuv420_from_rgba_pixmap_base(
                width,
                height,
                y_linesize,
                uv_linesize,
                uv_linesize,
                &rgba_pixels,
                y_pixels_base.as_mut_ptr(),
                cb_pixels_base.as_mut_ptr(),
                cr_pixels_base.as_mut_ptr(),
            );

            // Run NEON implementation
            fill_yuv420_from_rgba_pixmap_accelerated(
                width,
                height,
                y_linesize,
                uv_linesize,
                uv_linesize,
                &rgba_pixels,
                y_pixels_neon.as_mut_ptr(),
                cb_pixels_neon.as_mut_ptr(),
                cr_pixels_neon.as_mut_ptr(),
            );
        }

        // Compare results
        assert_eq!(y_pixels_base, y_pixels_neon, "Y planes differ");
        assert_eq!(cb_pixels_base, cb_pixels_neon, "Cb planes differ");
        assert_eq!(cr_pixels_base, cr_pixels_neon, "Cr planes differ");
    }
}
