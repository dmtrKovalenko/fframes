use std::path::Path;
use std::process::ExitCode;

use fframes::cli::{self, clap};
use fframes::{
    AudioMixOptions, CombinedMediaProvider, EncoderOptions, LimiterOptions, MediaDirectory,
    MediaProvider, RenderOptions, StaticMediaProvider,
};
use fframes_skia_renderer::{
    SkiaFFramesRenderer, SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig,
};
use overdrive_example::{HEIGHT, OverdriveMedia, Show, WIDTH};

#[derive(Debug, clap::Args)]
struct Args {
    /// Render with the built-in tiny-skia CPU backend instead of Skia on the GPU.
    #[arg(long, global = true)]
    cpu: bool,
}

fn main() -> ExitCode {
    let args = cli::parse::<Args>();
    let media = OverdriveMedia::prepare().expect("media");
    // `include_media_dir!` makes audio mono. The soundtrack is stereo, so it loads at run time.
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("dynamic_media");
    let dir = MediaDirectory::read_folder(&folder).expect(
        "dynamic_media: run `cargo run -p overdrive-example --bin overdrive-soundtrack` first",
    );
    let sounds = dir.process_media_source().expect("dynamic media");
    let all = CombinedMediaProvider::from([&media as &dyn MediaProvider, &sounds]);
    let video = Show::new();

    let options = RenderOptions {
        media: Some(&all),
        video_encoder_options: EncoderOptions {
            preferred_encoder: Some("libx264"),
            codec_params: Some(&[("crf", "17"), ("preset", "medium"), ("tune", "film")]),
            ..Default::default()
        },
        // The mix measures about -11.5 LUFS. This gain brings it to about -14 LUFS for the web.
        audio_mix: AudioMixOptions {
            master_gain_db: -2.5,
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
