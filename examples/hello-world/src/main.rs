use clap::Args;
use fframes::{EncoderOptions, RenderOptions, StaticMediaProvider, cli};
use fframes_skia_renderer::cli::SkiaRenderers;
use hello_world_example::{HelloWorldMedia, HelloWorldVideo};
use std::path::PathBuf;
use std::process::ExitCode;

/// Flags of this video, next to the standard ones of `fframes::cli`.
#[derive(Debug, Args)]
struct HelloWorldArgs {
    /// The text after "Hello".
    #[arg(long, default_value = "Renderer!", global = true)]
    slug: String,
}

fn main() -> ExitCode {
    let args = cli::parse::<HelloWorldArgs>();
    let media = HelloWorldMedia::prepare().unwrap();
    let video = HelloWorldVideo {
        media: &media,
        slug: &args.app.slug.clone(),
    };
    let tmp = PathBuf::from("test_render");

    cli::new(
        &video,
        RenderOptions {
            media: Some(&media),
            load_system_fonts: true,
            tmp_files_directory: Some(&tmp),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx264"),
                codec_params: Some(&[
                    ("crf", "23"),
                    ("preset", "ultrafast"),
                    ("tune", "animation"),
                ]),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .args(args)
    .renderers(SkiaRenderers::default())
    .run()
}
