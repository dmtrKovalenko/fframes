use fframes::{EncoderOptions, RenderOptions, StaticMediaProvider, cli};
use std::path::PathBuf;
use std::process::ExitCode;
use tiktok_example::{GooseMedia, GooseVideo};

fn main() -> ExitCode {
    // VP9 with alpha: the video has a transparent background.

    let media = GooseMedia::prepare().unwrap();
    let tmp = PathBuf::from("test_render");

    cli::new(
        &GooseVideo { media: &media },
        RenderOptions {
            media: Some(&media),
            tmp_files_directory: Some(&tmp),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libvpx-vp9"),
                pixel_format: fframes::AVPixelFormat::AV_PIX_FMT_YUVA420P,
                ..Default::default()
            },
            audio_encoder_options: EncoderOptions {
                preferred_encoder: Some("libopus"),
                sample_rate: 44100,
                bitrate: Some(128_000),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .backend(fframes::cpu::CpuRenderingBackend {
        cache_capacity: 200,
        ..Default::default()
    })
    .default_output("out.webm")
    .run()
}
