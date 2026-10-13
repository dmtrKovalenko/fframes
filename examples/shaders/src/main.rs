use fframes::cli::{self, clap};
use fframes::{EncoderOptions, RenderOptions, StaticMediaProvider};
use fframes_skia_renderer::{SkiaPipelineConfig, cli::SkiaRenderers};
use shaders_example::{ShadersMedia, ShadersVideo};
use std::process::ExitCode;

#[derive(Debug, clap::Args)]
struct Args {
    #[arg(long, default_value = "libx264", global = true)]
    video_codec: String,
}

fn main() -> ExitCode {
    let args = cli::parse::<Args>();
    let media = ShadersMedia::prepare().unwrap();
    let video = ShadersVideo::new(&media);
    let video_codec = args.app.video_codec.clone();

    // Shaders run on the Skia backend's own GPU surface; the tiny-skia CPU
    // backend (`--renderer cpu`) draws nothing in their place.
    cli::new(
        &video,
        RenderOptions {
            media: Some(&media),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some(&video_codec),
                codec_params: Some(&[("crf", "20"), ("preset", "slow")]),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .args(args)
    .preview(fframes_native_player::cli_preview)
    .renderers(SkiaRenderers::gpu(SkiaPipelineConfig::default()))
    .run()
}
