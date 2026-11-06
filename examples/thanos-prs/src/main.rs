mod audit;
mod hero;
mod motion;
mod sound;
mod ui;
mod video;

use std::process::ExitCode;

use fframes::cli;
use fframes::{
    AudioMixOptions, EncoderOptions, LimiterOptions, RenderOptions, StaticMediaProvider,
    include_media_dir,
};
use fframes_skia_renderer::{
    SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig, cli::SkiaRenderers,
};

use crate::video::SnapVideo;

include_media_dir!(struct SnapMedia, "examples/thanos-prs/media");

fn main() -> ExitCode {
    let media = SnapMedia::prepare().expect("load video media");
    let video = SnapVideo::new();

    cli::new(
        &video,
        RenderOptions {
            default_font: "Inter 24pt",
            media: Some(&media),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx264"),
                codec_params: Some(&[("crf", "17"), ("preset", "fast")]),
                ..Default::default()
            },
            audio_mix: AudioMixOptions {
                limiter: Some(LimiterOptions {
                    ceiling_db: -1.7,
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .preview(fframes_native_player::cli_preview)
    .renderers(SkiaRenderers::gpu(SkiaPipelineConfig {
        concurrency_policy: SkiaPipelineConcurrencyPolicy::MaxPerformance,
        ..Default::default()
    }))
    .default_output("out-thanos-prs.mp4")
    .run()
}
