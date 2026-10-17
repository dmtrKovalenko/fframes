use std::path::Path;
use std::process::ExitCode;

use arm_example::{ArmMedia, HEIGHT, Show, WIDTH};
use fframes::cli::{self, clap};
use fframes::{
    AudioMixOptions, CombinedMediaProvider, EncoderOptions, LimiterOptions, MediaDirectory,
    MediaProvider, RenderOptions, StaticMediaProvider,
};
use fframes_skia_renderer::{
    SkiaFFramesRenderer, SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig,
};

#[derive(Debug, clap::Args)]
struct Args {
    /// Render everything on the CPU: the 3D with the software rasterizer and the frame with
    /// the tiny-skia backend.
    #[arg(long, global = true)]
    cpu: bool,
    /// Read the 3D back from the GPU and upload it to Skia again. Use it to compare with the
    /// GPU texture.
    #[arg(long, global = true)]
    readback: bool,
}

fn main() -> ExitCode {
    let args = cli::parse::<Args>();
    let media = ArmMedia::prepare().expect("media");
    // `include_media_dir!` makes audio mono. The soundtrack is stereo, so it loads at run time.
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("dynamic_media");
    let dir = MediaDirectory::read_folder(&folder)
        .expect("dynamic_media: run `cargo run -p arm-example --bin arm-soundtrack` first");
    let sounds = dir.process_media_source().expect("dynamic media");
    let all = CombinedMediaProvider::from([&media as &dyn MediaProvider, &sounds]);
    let video = if args.app.cpu {
        Show::new().on_cpu()
    } else if args.app.readback {
        Show::new().read_back()
    } else {
        Show::new()
    };

    let options = RenderOptions {
        media: Some(&all),
        video_encoder_options: EncoderOptions {
            preferred_encoder: Some("libx264"),
            codec_params: Some(&[("crf", "17"), ("preset", "medium"), ("tune", "film")]),
            ..Default::default()
        },
        // The mix measures about -17.5 LUFS. This gain lifts it to about -14 LUFS.
        audio_mix: AudioMixOptions {
            master_gain_db: 6.5,
            limiter: Some(LimiterOptions {
                ceiling_db: -1.3,
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let cpu = args.app.cpu;
    let command = cli::new(&video, options)
        .args(args)
        .preview(fframes_native_player::cli_preview)
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
