use std::path::Path;

use audio_announce_example::{AudioAnnounce, AudioAnnounceMedia};
use clap::Parser;
use fframes::{CombinedMediaProvider, MediaProvider, StaticMediaProvider};
use fframes_renderer::{fframes_logger, render, EncoderOptions, RenderOptions};

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
    #[clap(short, long)]
    pub font: Option<String>,
}

fn main() {
    let args = Args::parse();
    let media = AudioAnnounceMedia::prepare().unwrap();
    let media_folder =
        fframes_renderer::MediaDirectory::read_folder(Path::new("./dynamic_media")).unwrap();
    let dynamic_media = media_folder.process_media_source().unwrap();

    render(
        &AudioAnnounce {
            media: &media,
            font: args.font.as_deref(),
        },
        args.output.as_str(),
        RenderOptions {
            media: Some(&CombinedMediaProvider::from([
                &media as &dyn MediaProvider,
                &dynamic_media as &dyn MediaProvider,
            ])),
            load_system_fonts: true,
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            encoder_options: EncoderOptions {
                preferred_audio_codec: args.audio_codec.as_deref(),
                preferred_video_codec: args.video_codec.as_deref(),
                codec_params: args
                    .output
                    .ends_with(".mp4")
                    // These are optimizations params set for the libx264 or libx265 encoders and handled directly by them.
                    .then_some(&[("crf", "18"), ("tune", "animation")]),
                ..Default::default()
            },
            render_backend: fframes_renderer::cpu::CpuRenderingBackend {
                cache_capacity: 300,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
