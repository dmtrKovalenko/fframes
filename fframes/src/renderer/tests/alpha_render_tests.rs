//! Renders half-transparent red over a transparent background, encodes it in color profiles
//! with and without alpha, and decodes the file again.

use crate::ffmpeg_sys_fframes::AVPixelFormat::{self, *};
use crate::media::FFmpegDecoder;
use crate::{
    AudioMap, Color, Duration, EncoderOptions, FFramesContext, Frame, RenderOptions, Svgr, Video,
    YuvMatrix, cpu::CpuRenderingBackend, fframes_logger::FFramesLoggerVariant,
};

#[derive(Debug)]
struct HalfRed;

impl Video for HalfRed {
    const FPS: usize = 10;
    const WIDTH: usize = 64;
    const HEIGHT: usize = 64;
    const BACKGROUND_COLOR: Color = Color::TRANSPARENT;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(2)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    // The renderer stores it premultiplied, as (128, 0, 0, 128).
    fn render_frame<'a>(&'a self, _frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        crate::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
                <rect width="64" height="64" fill="#f00" fill-opacity="0.5" />
            </svg>
        )
    }
}

/// The center pixel of the first frame. The decoder premultiplies videos with alpha, so a file
/// that stores straight colors gives back the pixels the renderer drew.
fn render_and_decode(
    encoder: &str,
    extension: &str,
    pixel_format: AVPixelFormat,
    matrix: YuvMatrix,
) -> [u8; 4] {
    let directory =
        std::env::temp_dir().join(format!("fframes-alpha-{encoder}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let output = directory.join(format!("half-red.{extension}"));

    crate::render(
        &output,
        &HalfRed,
        CpuRenderingBackend {
            concurrency: 1,
            ..Default::default()
        },
        &RenderOptions {
            logger: FFramesLoggerVariant::Silent,
            tmp_files_directory: Some(&directory.join("chunks")),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some(encoder),
                pixel_format,
                color_matrix: matrix,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap_or_else(|err| panic!("{encoder} {pixel_format:?}: {err}"));

    let pixel = unsafe {
        let mut decoder = FFmpegDecoder::new(&output, HalfRed::FPS, 1).unwrap();
        assert!(decoder.decode_up_to(0).unwrap());
        let image = decoder
            .get_raw_frame()
            .convert_last_decoded_frame_into_svg_image(None)
            .unwrap();
        let center = (32 * 64 + 32) * 4;
        image.data[center..center + 4].try_into().unwrap()
    };
    std::fs::remove_dir_all(directory).unwrap();
    pixel
}

fn assert_close(actual: [u8; 4], expected: [u8; 4], tolerance: u8, profile: &str) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(a, e)| a.abs_diff(e) <= tolerance),
        "{profile}: expected {expected:?}, decoded {actual:?}"
    );
}

#[test]
fn alpha_profiles_decode_to_the_rendered_pixels() {
    for (encoder, extension, pixel_format, matrix, tolerance) in [
        ("qtrle", "mov", AV_PIX_FMT_ARGB, YuvMatrix::Bt601, 0),
        ("ffv1", "mkv", AV_PIX_FMT_YUVA420P, YuvMatrix::Bt601, 3),
        ("ffv1", "mkv", AV_PIX_FMT_YUVA420P, YuvMatrix::Bt709, 3),
        ("ffv1", "mkv", AV_PIX_FMT_YUVA444P, YuvMatrix::Bt601, 3),
        ("ffv1", "mkv", AV_PIX_FMT_YUVA444P, YuvMatrix::Bt709, 3),
        (
            "prores_ks",
            "mov",
            AV_PIX_FMT_YUVA444P10LE,
            YuvMatrix::Bt709,
            4,
        ),
    ] {
        let profile = format!("{encoder} {pixel_format:?} {matrix:?}");
        let pixel = render_and_decode(encoder, extension, pixel_format, matrix);
        // stored premultiplied, the red would decode as 64
        assert_close(pixel, [128, 0, 0, 128], tolerance, &profile);
    }
}

#[test]
fn profiles_without_alpha_composite_over_black() {
    for (pixel_format, matrix) in [
        (AV_PIX_FMT_YUV420P, YuvMatrix::Bt601),
        (AV_PIX_FMT_YUV420P, YuvMatrix::Bt709),
        (AV_PIX_FMT_YUV444P, YuvMatrix::Bt709),
    ] {
        let profile = format!("ffv1 {pixel_format:?} {matrix:?}");
        let pixel = render_and_decode("ffv1", "mkv", pixel_format, matrix);
        assert_close(pixel, [128, 0, 0, 255], 3, &profile);
    }
}
