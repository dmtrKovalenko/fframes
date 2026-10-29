use std::process::ExitCode;

use fframes::cli::{self, clap};
use fframes::{RenderOptions, StaticMediaProvider};
use fframes_skia_renderer::{
    SkiaFFramesRenderer, SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig,
};
use svg_in_3d_example::{HEIGHT, Monitor, SvgIn3dMedia, WIDTH};

#[derive(Debug, clap::Args)]
struct Args {
    /// Render everything on the CPU: the 3D with the software rasterizer, the svgr images
    /// and the frame with the tiny-skia backend.
    #[arg(long, global = true)]
    cpu: bool,
}

fn main() -> ExitCode {
    let args = cli::parse::<Args>();
    let media = SvgIn3dMedia::prepare().expect("media");
    let cpu = args.app.cpu;
    let video = if cpu {
        Monitor::new().on_cpu()
    } else {
        Monitor::new()
    };
    let options = RenderOptions {
        media: Some(&media),
        ..Default::default()
    };

    let command = cli::new(&video, options)
        .args(args)
        .default_output("out.mp4");
    if cpu {
        return command.run();
    }

    #[cfg(target_os = "macos")]
    let gpu = fframes_skia_renderer::metal::SkiaMetalCtx::new(WIDTH, HEIGHT).expect("GPU context");
    #[cfg(not(target_os = "macos"))]
    let gpu =
        fframes_skia_renderer::vulkan::SkiaVulkanCtx::new(WIDTH, HEIGHT).expect("GPU context");
    let pipeline = SkiaPipelineConfig {
        concurrency_policy: SkiaPipelineConcurrencyPolicy::MaxPerformance,
        ..Default::default()
    };
    #[cfg(target_os = "macos")]
    let backend = SkiaFFramesRenderer::new_metal(&gpu, pipeline);
    #[cfg(not(target_os = "macos"))]
    let backend = SkiaFFramesRenderer::new_vulkan(&gpu, pipeline);
    command.backend(backend.expect("skia renderer")).run()
}
