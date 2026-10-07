use std::process::ExitCode;

use fframes::cli::clap;
use fframes::{
    AudioMixOptions, CombinedMediaProvider, EncoderOptions, MediaDirectory, MediaProvider,
    RenderOptions, StaticMediaProvider, Video, cli,
};
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConfig};
use made_of_motion::{MadeOfMotion, MotionMedia};

#[derive(Debug, cli::clap::Args)]
struct Args {
    /// Inspect only the frame-by-frame SVG ink, without objects or footage.
    #[arg(long, global = true)]
    ink_only: bool,
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let args = cli::parse::<Args>();
    let media = MotionMedia::prepare()?;
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dynamic_media");
    for file in ["portrait-clean.mp4", "soundtrack.wav"] {
        if !folder.join(file).is_file() {
            return Err(format!(
                "Missing {}. Restore the final media supplied with this example; see README.md.",
                folder.join(file).display()
            )
            .into());
        }
    }
    let directory = MediaDirectory::read_folder(folder)?;
    let dynamic = directory.process_media_source()?;
    let all = CombinedMediaProvider::from([&media as &dyn MediaProvider, &dynamic]);
    let video = MadeOfMotion::new().with_ink_only(args.app.ink_only);
    #[cfg(target_os = "macos")]
    let gpu =
        fframes_skia_renderer::metal::SkiaMetalCtx::new(MadeOfMotion::WIDTH, MadeOfMotion::HEIGHT)?;
    #[cfg(not(target_os = "macos"))]
    let gpu = fframes_skia_renderer::vulkan::SkiaVulkanCtx::new(
        MadeOfMotion::WIDTH,
        MadeOfMotion::HEIGHT,
    )?;
    #[cfg(target_os = "macos")]
    let backend = SkiaFFramesRenderer::new_metal(&gpu, SkiaPipelineConfig::default())?;
    #[cfg(not(target_os = "macos"))]
    let backend = SkiaFFramesRenderer::new_vulkan(&gpu, SkiaPipelineConfig::default())?;
    Ok(cli::new(
        &video,
        RenderOptions {
            media: Some(&all),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx264"),
                codec_params: Some(&[("crf", "16"), ("preset", "medium")]),
                ..Default::default()
            },
            audio_mix: AudioMixOptions {
                limiter: None,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .args(args)
    .backend(backend)
    .preview(fframes_native_player::cli_preview)
    .default_output(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("output/made-of-motion.mp4"),
    )
    .run())
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("made-of-motion: {error}");
            ExitCode::FAILURE
        }
    }
}
