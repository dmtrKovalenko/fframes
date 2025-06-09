use std::path::PathBuf;

use fframes::{EncoderOptions, RenderOptions, StaticMediaProvider, fframes_logger, render};
use tiktok_example::{GooseMedia, GooseVideo};

fn main() {
    let media = GooseMedia::prepare().unwrap();

    render(
        "out.webm",
        &GooseVideo { media: &media },
        fframes::cpu::CpuRenderingBackend {
            cache_capacity: 20,
            ..Default::default()
        },
        &RenderOptions {
            media: Some(&media),
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            tmp_files_directory: Some(&PathBuf::from("test_render")),
            video_encoder_options: EncoderOptions {
                pixel_format: fframes::AVPixelFormat::AV_PIX_FMT_YUVA420P,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
