#![cfg(feature = "vulkan")]

use fframes::ffmpeg_sys_fframes::AVPixelFormat::AV_PIX_FMT_YUV420P;
use fframes::media::FFmpegDecoder;
use fframes::{
    AudioMap, Color, Duration, EncoderOptions, FFramesContext, Frame, RenderOptions, Svgr, Video,
};
use fframes_skia_renderer::vulkan::SkiaVulkanCtx;
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaFrameExport, SkiaPipelineConfig};

const WIDTH: u32 = 322;
const HEIGHT: u32 = 242;

/// `None` on machines without a Vulkan device (the tests are skipped there).
fn vulkan() -> Option<SkiaVulkanCtx> {
    match SkiaVulkanCtx::new(WIDTH as usize, HEIGHT as usize) {
        Ok(ctx) => Some(ctx),
        Err(err) => {
            eprintln!("skipping: no Vulkan device ({err:?})");
            None
        }
    }
}

const FPS: usize = 10;
const FRAMES: usize = 30;
const SWITCH_AT: usize = FRAMES / 2;

#[derive(Debug)]
struct TwoColors;

impl Video for TwoColors {
    const FPS: usize = FPS;
    const WIDTH: usize = WIDTH as usize;
    const HEIGHT: usize = HEIGHT as usize;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(FRAMES)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let fill = if frame.index < SWITCH_AT {
            "#ff0000"
        } else {
            "#0000ff"
        };

        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={Self::WIDTH} height={Self::HEIGHT}>
                <rect x="0" y="0" width={Self::WIDTH} height={Self::HEIGHT} fill={fill} />
            </svg>
        )
    }
}

fn center_pixel(decoder: &mut FFmpegDecoder, frame: usize) -> [u8; 4] {
    unsafe {
        assert!(
            decoder.decode_up_to(frame as i64).unwrap(),
            "frame {frame} is missing from the rendered video"
        );

        let image = decoder
            .get_raw_frame()
            .convert_last_decoded_frame_into_svg_image(None)
            .unwrap();
        let offset =
            ((image.height as usize / 2) * image.width as usize + image.width as usize / 2) * 4;
        image.data[offset..offset + 4].try_into().unwrap()
    }
}

fn assert_color_close(actual: [u8; 4], expected: [u8; 3], frame: usize) {
    // lossy yuv420p encoding shifts solid colors by a few units
    let close = actual[..3]
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= 24);
    assert!(
        close,
        "frame {frame}: expected ~{expected:?}, decoded {actual:?}"
    );
}

#[test]
fn renders_videos_through_every_export_mode() {
    let Some(vulkan) = vulkan() else { return };

    for (mode, pixel_format) in [
        (SkiaFrameExport::Auto, AV_PIX_FMT_YUV420P),
        (SkiaFrameExport::GpuConversion, AV_PIX_FMT_YUV420P),
        (SkiaFrameExport::CpuConversion, AV_PIX_FMT_YUV420P),
    ] {
        let dir = std::env::temp_dir().join(format!(
            "fframes-skia-export-{}-{mode:?}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let output = dir.join("out.mp4");

        fframes::render(
            &output,
            &TwoColors,
            SkiaFFramesRenderer::new_vulkan(&vulkan, SkiaPipelineConfig::default())
                .unwrap()
                .frame_export(mode),
            &RenderOptions {
                logger: fframes::fframes_logger::FFramesLoggerVariant::Silent,
                tmp_files_directory: Some(&dir.join("chunks")),
                video_encoder_options: EncoderOptions {
                    // decodes in software whatever FFmpeg was built with
                    preferred_encoder: Some("mpeg4"),
                    pixel_format,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap_or_else(|err| panic!("{mode:?}: {err:?}"));

        let mut decoder = unsafe { FFmpegDecoder::new(&output, FPS, 1) }.unwrap();
        assert_color_close(center_pixel(&mut decoder, 0), [255, 0, 0], 0);
        assert_color_close(
            center_pixel(&mut decoder, SWITCH_AT + 2),
            [0, 0, 255],
            SWITCH_AT + 2,
        );
        drop(decoder);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn h264_renders_decode_with_or_without_a_hardware_decoder() {
    let Some(vulkan) = vulkan() else { return };
    let encoder_options = EncoderOptions {
        preferred_encoder: Some("libx264"),
        ..Default::default()
    };
    let dir = std::env::temp_dir().join(format!("fframes-skia-export-{}-h264", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let output = dir.join("out.mp4");

    let encoder = fframes::VideoEncoderInfo::for_output(
        &output,
        (WIDTH as i32, HEIGHT as i32, FPS as i32),
        &encoder_options,
    )
    .unwrap();
    if encoder.name() != "libx264" {
        eprintln!("skipping: FFmpeg was built without libx264 (the `h264` feature)");
        return;
    }

    fframes::render(
        &output,
        &TwoColors,
        SkiaFFramesRenderer::new_vulkan(&vulkan, SkiaPipelineConfig::default()).unwrap(),
        &RenderOptions {
            logger: fframes::fframes_logger::FFramesLoggerVariant::Silent,
            tmp_files_directory: Some(&dir.join("chunks")),
            video_encoder_options: encoder_options,
            ..Default::default()
        },
    )
    .unwrap();

    // With FFmpeg's Vulkan support compiled in the decoder asks the GPU first and has to
    // fall back to software on drivers without H.264 decode.
    let mut decoder = unsafe { FFmpegDecoder::new(&output, FPS, 1) }.unwrap();
    assert_color_close(center_pixel(&mut decoder, 0), [255, 0, 0], 0);
    assert_color_close(
        center_pixel(&mut decoder, SWITCH_AT + 2),
        [0, 0, 255],
        SWITCH_AT + 2,
    );
    drop(decoder);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_encoder_that_does_not_open_fails_the_render() {
    let Some(vulkan) = vulkan() else { return };
    let dir = std::env::temp_dir().join(format!("fframes-skia-export-{}-bad", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // The encoder is opened by the encoding threads. When they give up, the stages
    // feeding them have to stop too instead of waiting for room in the queues.
    let result = fframes::render(
        dir.join("out.mp4"),
        &TwoColors,
        SkiaFFramesRenderer::new_vulkan(&vulkan, SkiaPipelineConfig::default()).unwrap(),
        &RenderOptions {
            logger: fframes::fframes_logger::FFramesLoggerVariant::Silent,
            tmp_files_directory: Some(&dir.join("chunks")),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("mpeg4"),
                // a maximum bitrate needs a buffer size, which is not set
                codec_params: Some(&[("maxrate", "1000")]),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        result.is_err(),
        "the encoder was expected to reject its options"
    );
}

#[cfg(feature = "vulkan-video")]
mod vulkan_video {
    use super::*;
    use fframes::FFramesRenderBackend;

    #[derive(Debug)]
    struct TwoColors720;

    impl Video for TwoColors720 {
        const FPS: usize = 30;
        const WIDTH: usize = 1280;
        const HEIGHT: usize = 720;
        const BACKGROUND_COLOR: Color = Color::BLACK;

        fn duration(&self) -> Duration<'_> {
            Duration::Frames(60)
        }

        fn audio(&self) -> AudioMap<'_> {
            AudioMap::none()
        }

        fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
            let fill = if frame.index < 30 {
                "#ff0000"
            } else {
                "#0000ff"
            };

            fframes::svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" width={Self::WIDTH} height={Self::HEIGHT}>
                    <rect x="0" y="0" width={Self::WIDTH} height={Self::HEIGHT} fill={fill} />
                </svg>
            )
        }
    }

    /// Renders through a Vulkan Video encoder and checks the decoded colors. Skipped where
    /// the driver has no such encoder.
    fn renders_through(encoder: &str) {
        let Ok(vulkan) =
            SkiaVulkanCtx::new_shared_with_encoder(TwoColors720::WIDTH, TwoColors720::HEIGHT)
        else {
            eprintln!("skipping: FFmpeg has no Vulkan device");
            return;
        };

        let dir = std::env::temp_dir().join(format!(
            "fframes-skia-export-{}-{encoder}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let output = dir.join("out.mp4");
        let options = RenderOptions {
            logger: fframes::fframes_logger::FFramesLoggerVariant::Silent,
            tmp_files_directory: Some(&dir.join("chunks")),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some(encoder),
                // Mesa's Intel driver (25.1, `ANV_DEBUG=video-encode`) writes HEVC that
                // decodes washed out below the quantizer 26, whatever feeds the encoder.
                codec_params: Some(&[("qp", "26")]),
                ..Default::default()
            },
            ..Default::default()
        };

        let info = fframes::VideoEncoderInfo::for_output(
            &output,
            (
                TwoColors720::WIDTH as i32,
                TwoColors720::HEIGHT as i32,
                TwoColors720::FPS as i32,
            ),
            &options.video_encoder_options,
        )
        .unwrap();
        let backend =
            SkiaFFramesRenderer::new_vulkan(&vulkan, SkiaPipelineConfig::default()).unwrap();
        let hardware = info.name() == encoder
            && backend
                .negotiate_encoder_input(&info)
                .is_ok_and(|input| input.is_hardware());
        if !hardware {
            eprintln!("skipping {encoder}: the driver has no such encoder");
            return;
        }

        fframes::render(&output, &TwoColors720, backend, &options)
            .unwrap_or_else(|err| panic!("{encoder}: {err:?}"));

        let mut decoder = unsafe { FFmpegDecoder::new(&output, TwoColors720::FPS, 1) }.unwrap();
        assert_color_close(center_pixel(&mut decoder, 0), [255, 0, 0], 0);
        assert_color_close(center_pixel(&mut decoder, 32), [0, 0, 255], 32);
        drop(decoder);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hevc_vulkan_reads_the_frames_skia_rendered() {
        renders_through("hevc_vulkan");
    }

    #[test]
    fn h264_vulkan_reads_the_frames_skia_rendered() {
        renders_through("h264_vulkan");
    }
}
