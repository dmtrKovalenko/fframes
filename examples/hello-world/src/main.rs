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
    #[clap(long)]
    video_codec: Option<String>,
    #[clap(long)]
    audio_codec: Option<String>,
    #[clap(short, long)]
    concurrency: Option<usize>,
    #[clap(long, default_value = "Hello Renderer!")]
    slug: String,
}

fn main() {
    let args = Args::parse();
    let media = HelloWorldMedia::prepare().unwrap();

    render(
        &HelloWorldVideo {
            media: &media,
            slug: &args.slug,
        },
        args.output.as_str(),
        RenderOptions {
            media: Some(&media),
            load_system_fonts: true,
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            encoder_options: EncoderOptions {
                preferred_audio_codec: args.audio_codec.as_deref(),
                preferred_video_codec: args.video_codec.as_deref(),
                tmp_files_directory: Some(&PathBuf::from("test_render")),
                codec_params: args
                    .output
                    .ends_with(".mp4")
                    // These are optimizations params set for the libx264 or libx265 encoders and handled directly by them.
                    .then_some(&[("crf", "18"), ("tune", "animation")]),
                ..Default::default()
            },
            render_backend: if let Some(concurrency) = args.concurrency {
                fframes_renderer::cpu::CpuRenderingBackend {
                    concurrency,
                    cache_capacity: 5,
                    ..Default::default()
                }
            } else {
                fframes_renderer::cpu::CpuRenderingBackend {
                    cache_capacity: 0,
                    ..Default::default()
                }
            },
            ..Default::default()
        },
    )
    .unwrap();
}
