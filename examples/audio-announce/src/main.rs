use audio_announce_example::{AudioAnnounce, AudioAnnounceMedia};
use fframes::cli::{self, clap};
use fframes::{
    CombinedMediaProvider, EncoderOptions, MediaProvider, RenderOptions, StaticMediaProvider, Video,
};
use fframes_skia_renderer::vulkan::SkiaVulkanCtx;
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConfig};
use std::process::ExitCode;

#[derive(Debug, clap::Args)]
struct Args {
    /// Font family of the text.
    #[arg(short, long, global = true)]
    font: Option<String>,
}

fn main() -> ExitCode {
    let args = cli::parse::<Args>();
    let media = AudioAnnounceMedia::prepare().unwrap();
    let media_folder = fframes::MediaDirectory::read_folder("./dynamic_media").unwrap();
    let dynamic_media = media_folder.process_media_source().unwrap();
    let all_media = CombinedMediaProvider::from([
        &media as &dyn MediaProvider,
        &dynamic_media as &dyn MediaProvider,
    ]);
    let font = args.app.font.clone();

    let vulkan_ctx = SkiaVulkanCtx::new(AudioAnnounce::WIDTH, AudioAnnounce::HEIGHT).unwrap();

    cli::new(
        &AudioAnnounce {
            media: &media,
            font: font.as_deref(),
        },
        RenderOptions {
            media: Some(&all_media),
            load_system_fonts: true,
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx264"),
                qmin: 0,
                qmax: 69,
                qcompress: 0.6,
                max_qdiff: 4,
                gop_size: 60,
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
    .args(args)
    .backend(
        // Swap in `fframes::cpu::CpuRenderingBackend::default()` to compare with the CPU.
        SkiaFFramesRenderer::new_vulkan(
            &vulkan_ctx,
            SkiaPipelineConfig {
                buffer_queue_size: AudioAnnounce::FPS,
                // this works well on the author's machine, test other values on yours
                concurrency_policy:
                    fframes_skia_renderer::SkiaPipelineConcurrencyPolicy::Concurrency(4),
                ..Default::default()
            },
        )
        .expect("Failed to create the skia renderer"),
    )
    .preview(fframes_native_player::cli_preview)
    .run()
}
