use audio_announce_example::{AudioAnnounce, AudioAnnounceMedia};
use clap::Parser;
use fframes::{CombinedMediaProvider, MediaProvider, StaticMediaProvider, Video};
use fframes_renderer::{fframes_logger, render, EncoderOptions, RenderOptions};
use fframes_skia_renderer::vulkan::SkiaVulkanCtx;
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConfig};
use std::path::Path;

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

    let vulkan_ctx = SkiaVulkanCtx::new(AudioAnnounce::WIDTH, AudioAnnounce::HEIGHT).unwrap();

    render(
        args.output.as_str(),
        &AudioAnnounce {
            media: &media,
            font: args.font.as_deref(),
        },
        SkiaFFramesRenderer::new_vulkan(
            &vulkan_ctx,
            SkiaPipelineConfig {
                buffer_queue_size: 2,
                encoder_threads: 1,
            },
        )
        .expect("Failed to create metal renderer"),
        &RenderOptions {
            media: Some(&CombinedMediaProvider::from([
                &media as &dyn MediaProvider,
                &dynamic_media as &dyn MediaProvider,
            ])),
            load_system_fonts: true,
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            encoder_options: EncoderOptions {
                preferred_video_codec: Some("libx264"),
                qmin: 0,
                qmax: 69,
                qcompress: 0.6,
                max_qdiff: 4,
                gop_size: 250,
                codec_params: Some(&[
                    ("crf", "18"),
                    ("preset", "slow"),
                    ("tune", "film"),
                    (
                        "x264-params",
                        "aq-mode=3:aq-strength=0.8:deblock=1,1:psy-rd=1.0:psy-rdoq=2.0:rdoq-level=2:merange=32"
                    ),
                    ("bframes", "3"),
                ]),
                ..Default::default()
            },
                ..Default::default()
        },
    )
    .unwrap();
}
