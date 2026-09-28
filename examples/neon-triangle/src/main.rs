use fframes::cli::{self, clap};
use fframes::{EncoderOptions, RenderOptions, StaticMediaProvider, Video};
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConfig, vulkan::SkiaVulkanCtx};
use neon_triangle_example::{NeonTriangleMedia, NeonTriangleVideo};
use std::process::ExitCode;

#[derive(Debug, clap::Args)]
struct Args {
    #[arg(long, default_value = "libx264", global = true)]
    video_codec: String,
}

fn main() -> ExitCode {
    let args = cli::parse::<Args>();
    let media = NeonTriangleMedia::prepare().unwrap();
    let video = NeonTriangleVideo::new(&media);
    let video_codec = args.app.video_codec.clone();

    // Shaders run on the Skia backend's own GPU surface; the tiny-skia CPU
    // backend would draw nothing in their place, so previews use Skia too.
    let vulkan_ctx =
        SkiaVulkanCtx::new(NeonTriangleVideo::WIDTH, NeonTriangleVideo::HEIGHT).unwrap();

    cli::new(
        &video,
        RenderOptions {
            media: Some(&media),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some(&video_codec),
                codec_params: Some(&[("crf", "16"), ("preset", "slow")]),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .args(args)
    .backend(
        SkiaFFramesRenderer::new_vulkan(&vulkan_ctx, SkiaPipelineConfig::default())
            .expect("Failed to create renderer"),
    )
    .preview(fframes_native_player::cli_preview)
    .run()
}
