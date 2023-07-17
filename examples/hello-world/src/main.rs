use clap::Parser;
use fframes_renderer::{fframes_logger, render, render_backend, EncoderOptions, RenderOptions};
use hello_world_example::HelloWorldVideo;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
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
    render(
        HelloWorldVideo { slug: &args.slug },
        args.output.as_str(),
        RenderOptions {
            media_dir: "./media",
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
                render_backend::CpuRenderingBackend {
                    concurrency,
                    cache_capacity: 5,
                    ..Default::default()
                }
            } else {
                render_backend::CpuRenderingBackend {
                    cache_capacity: 5,
                    ..Default::default()
                }
            },
            ..Default::default()
        },
    )
    .unwrap();
}
