use clap::Parser;
use fframes::StaticMediaProvider;
use fframes_renderer::{fframes_logger, render, EncoderOptions, RenderOptions};
use hello_world_example::{HelloWorldMedia, HelloWorldVideo};
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[clap(short, long)]
    gpu: bool,
    #[clap(short, long, default_value = "out.mp4")]
    output: String,
    #[clap(long, default_value = "libx264")]
    video_codec: Option<String>,
    #[clap(long)]
    audio_codec: Option<String>,
    #[clap(short, long)]
    concurrency: Option<usize>,
    #[clap(long, default_value = "Hello Renderer!")]
    slug: String,
    #[clap(short, long)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();
    let media = HelloWorldMedia::prepare().unwrap();

    render(
        "out.mp4",
        &HelloWorldVideo {
            media: &media,
            slug: &args.slug,
        },
        if let Some(concurrency) = args.concurrency {
            fframes_renderer::cpu::CpuRenderingBackend {
                concurrency,
                cache_capacity: 5,
                ..Default::default()
            }
        } else {
            fframes_renderer::cpu::CpuRenderingBackend {
                cache_capacity: 5,
                ..Default::default()
            }
        },
        &RenderOptions {
            media: Some(&media),
            load_system_fonts: true,
            logger: if args.verbose {
                fframes_logger::FFramesLoggerVariant::Debug
            } else {
                fframes_logger::FFramesLoggerVariant::Compact
            },
            encoder_options: EncoderOptions {
                preferred_audio_codec: args.audio_codec.as_deref(),
                preferred_video_codec: args.video_codec.as_deref(),
                tmp_files_directory: Some(&PathBuf::from("test_render")),
                codec_params: (args.video_codec.as_deref() == Some("libx264")
                    || args.video_codec.as_deref() == Some("libx265"))
                .then_some(&[
                    ("crf", "23"),
                    ("preset", "ultrafast"),
                    ("tune", "animation"),
                ]),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
