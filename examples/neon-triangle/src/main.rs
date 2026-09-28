use clap::Parser;
use fframes::{EncoderOptions, RenderOptions, StaticMediaProvider, Video, fframes_logger};
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConfig, vulkan::SkiaVulkanCtx};
use image::{ImageBuffer, Rgba};
use neon_triangle_example::{NeonTriangleMedia, NeonTriangleVideo};

#[derive(Debug, Parser)]
struct Args {
    #[clap(short, long, default_value = "out.mp4")]
    output: String,
    #[clap(long, default_value = "libx264")]
    video_codec: Option<String>,
    #[clap(short, long)]
    verbose: bool,
    /// Render a single frame to PNG instead of the full video
    #[clap(long)]
    preview: bool,
    /// Frame index to render with --preview
    #[clap(long, default_value = "90")]
    preview_frame: usize,
}

fn main() {
    let args = Args::parse();
    let media = NeonTriangleMedia::prepare().unwrap();
    let video = NeonTriangleVideo::new(&media);

    // Shaders run on the Skia backend's own GPU surface; the tiny-skia CPU
    // backend would draw nothing in their place.
    let vulkan_ctx =
        SkiaVulkanCtx::new(NeonTriangleVideo::WIDTH, NeonTriangleVideo::HEIGHT).unwrap();
    let backend = SkiaFFramesRenderer::new_vulkan(&vulkan_ctx, SkiaPipelineConfig::default())
        .expect("Failed to create renderer");

    let options = RenderOptions {
        media: Some(&media),
        logger: if args.verbose {
            fframes_logger::FFramesLoggerVariant::Debug
        } else {
            fframes_logger::FFramesLoggerVariant::Compact
        },
        video_encoder_options: EncoderOptions {
            preferred_encoder: args.video_codec.as_deref(),
            codec_params: Some(&[("crf", "16"), ("preset", "slow")]),
            ..Default::default()
        },
        ..Default::default()
    };

    if args.preview {
        let pixels = fframes::render_frame(args.preview_frame, &video, backend, &options)
            .expect("Failed to render preview frame");
        ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(
            NeonTriangleVideo::WIDTH as u32,
            NeonTriangleVideo::HEIGHT as u32,
            pixels,
        )
        .expect("Failed to create image buffer")
        .save("frame_preview.png")
        .expect("Failed to save preview image");

        println!("Saved frame {} to frame_preview.png", args.preview_frame);
    } else {
        fframes::render(args.output.as_str(), &video, backend, &options).unwrap();
    }
}
