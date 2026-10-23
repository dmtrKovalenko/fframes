use crate::RgbaFrameConverter;
use crate::media::ffmpeg_sys_fframes::AVPixelFormat;
use crate::renderer::pix_fmt::YuvMatrix;

const SIZE: i32 = 8;

fn solid_frame(format: AVPixelFormat, matrix: YuvMatrix, rgba: [u8; 4]) -> crate::VideoFrame {
    let mut converter = RgbaFrameConverter::with_matrix(format, matrix, SIZE, SIZE).unwrap();
    converter
        .convert(&rgba.repeat((SIZE * SIZE) as usize))
        .unwrap()
}

#[test]
fn yuva_keeps_straight_colors_and_alpha() {
    for matrix in [YuvMatrix::Bt601, YuvMatrix::Bt709] {
        for format in [
            AVPixelFormat::AV_PIX_FMT_YUVA444P,
            AVPixelFormat::AV_PIX_FMT_YUVA420P,
        ] {
            let translucent = solid_frame(format, matrix, [128, 0, 0, 128]);
            let opaque = solid_frame(format, matrix, [255, 0, 0, 255]);
            let colors =
                |frame: &crate::VideoFrame| [0, 1, 2].map(|plane| frame.copy_plane(plane, 1, 1)[0]);
            assert_eq!(
                colors(&translucent),
                colors(&opaque),
                "half-transparent red must keep its color in {format:?} ({matrix:?})"
            );
            // swscale can round the alpha sample by one level.
            for alpha in translucent.copy_plane(3, SIZE as usize, SIZE as usize) {
                assert!(alpha.abs_diff(128) <= 1, "alpha: {alpha}");
            }
        }
    }
}

#[test]
fn rgba_unpremultiplies_each_pixel_without_changing_the_source() {
    let mut converter = RgbaFrameConverter::new(AVPixelFormat::AV_PIX_FMT_RGBA, 3, 2).unwrap();
    let pixels = [
        128, 0, 0, 128, // half-transparent red
        0, 0, 0, 0, // transparent
        17, 85, 153, 255, // opaque
        0, 1, 0, 1, // nearly transparent green
        16, 32, 48, 64, // mixed channels, with rounding
        0, 0, 0, 128, // half-transparent black
    ];
    let original = pixels;
    let frame = converter.convert(&pixels).unwrap();
    assert_eq!(
        frame.copy_plane(0, 3 * 4, 2),
        [
            255, 0, 0, 128, 0, 0, 0, 0, 17, 85, 153, 255, 0, 255, 0, 1, 64, 128, 191, 64, 0, 0, 0,
            128,
        ]
    );
    assert_eq!(pixels, original);

    // Reusing the converter must replace its scratch pixels and leave earlier frames intact.
    let next = [0, 0, 64, 64].repeat(6);
    let next_frame = converter.convert(&next).unwrap();
    assert_eq!(
        next_frame.copy_plane(0, 3 * 4, 2),
        [0, 0, 255, 64].repeat(6)
    );
    assert_eq!(frame.copy_plane(0, 4, 1), [255, 0, 0, 128]);
}

#[test]
fn bgra_output_unpremultiplies_before_swscale_reorders_channels() {
    let frame = solid_frame(
        AVPixelFormat::AV_PIX_FMT_BGRA,
        YuvMatrix::Bt601,
        [64, 32, 0, 64],
    );
    assert_eq!(frame.copy_plane(0, 4, 1), [0, 128, 255, 64]);
}

#[test]
fn non_alpha_outputs_keep_colors_composited_over_black() {
    for matrix in [YuvMatrix::Bt601, YuvMatrix::Bt709] {
        for format in [
            AVPixelFormat::AV_PIX_FMT_YUV420P,
            AVPixelFormat::AV_PIX_FMT_YUV444P,
        ] {
            let translucent = solid_frame(format, matrix, [128, 32, 64, 128]);
            let opaque = solid_frame(format, matrix, [128, 32, 64, 255]);
            for plane in 0..3 {
                let size = if format == AVPixelFormat::AV_PIX_FMT_YUV420P && plane != 0 {
                    SIZE as usize / 2
                } else {
                    SIZE as usize
                };
                assert_eq!(
                    translucent.copy_plane(plane, size, size),
                    opaque.copy_plane(plane, size, size),
                    "{format:?} ({matrix:?}), plane {plane}"
                );
            }
        }
    }
}

#[cfg(feature = "cpu_renderer")]
#[test]
fn frame_renderer_adapter_does_not_unpremultiply_straight_pixels_again() {
    use crate::{
        Color, CpuFrameRenderer, EncoderFrameRenderer, EncoderInput, FrameRenderer,
        RgbaEncoderFrameRenderer,
    };

    let tree = crate::svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8" />)
        .into_svg_tree(
            &crate::usvgr::Options::default(),
            &mut crate::usvgr::Cache::default(),
            &crate::usvgr::fontdb::Database::new(),
        )
        .unwrap();
    let background = Color::rgba(64, 128, 192, 128);
    let expected = CpuFrameRenderer::default()
        .render_tree(&tree, background, SIZE as u32, SIZE as u32)
        .unwrap();
    let input = EncoderInput::software(AVPixelFormat::AV_PIX_FMT_RGBA);
    let mut adapter = RgbaEncoderFrameRenderer::new(
        CpuFrameRenderer::default(),
        &input,
        SIZE as u32,
        SIZE as u32,
    )
    .unwrap();
    let frame = adapter.render_tree(&tree, background).unwrap();
    assert_eq!(
        frame.copy_plane(0, SIZE as usize * 4, SIZE as usize),
        expected.pixels
    );

    let mut native =
        crate::cpu::CpuEncoderFrameRenderer::new(0, &input, SIZE as u32, SIZE as u32).unwrap();
    let frame = native.render_tree(&tree, background).unwrap();
    assert_eq!(
        frame.copy_plane(0, SIZE as usize * 4, SIZE as usize),
        expected.pixels
    );
}
