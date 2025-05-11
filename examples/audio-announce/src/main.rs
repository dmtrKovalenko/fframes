use audio_announce_example::{AudioAnnounce, AudioAnnounceMedia};
use clap::Parser;
use fframes::{
    CombinedMediaProvider, EncoderOptions, MediaProvider, RenderOptions, StaticMediaProvider,
    Video, fframes_logger,
};
use fframes_skia_renderer::vulkan::SkiaVulkanCtx;
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConfig};
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
    #[clap(short, long)]
    pub font: Option<String>,
}

fn main() {
    let args = Args::parse();
    let media = AudioAnnounceMedia::prepare().unwrap();
    let media_folder = fframes::MediaDirectory::read_folder("./dynamic_media").unwrap();
    let dynamic_media = media_folder.process_media_source().unwrap();

    let vulkan_ctx = SkiaVulkanCtx::new(AudioAnnounce::WIDTH, AudioAnnounce::HEIGHT).unwrap();

    fframes::render(
        args.output.as_str(),
        &AudioAnnounce {
            media: &media,
            font: args.font.as_deref(),
        },
        // Render it with CPU to see the difference
        // fframes::cpu::CpuRenderingBackend {
        //     ..Default::default()
        // },
        SkiaFFramesRenderer::new_vulkan(
            &vulkan_ctx,
            SkiaPipelineConfig {
                buffer_queue_size: AudioAnnounce::FPS,
                concurrency_policy:
                    // this works perfectly on my machine but you should test different options
                    fframes_skia_renderer::SkiaPipelineConcurrencyPolicy::Concurrency(4),
                ..Default::default()
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
                gop_size: 60,
                tmp_files_directory: Some(&PathBuf::from("test_render")),
                codec_params: Some(&[
                    ("crf", "25"),
                    ("preset", "slower"),
                    ("tune", "film"),
                    ("bframes", "3"),
                ]),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
