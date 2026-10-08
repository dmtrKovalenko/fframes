use std::process::ExitCode;

use fframes::{
    CombinedMediaProvider, EncoderOptions, MediaDirectory, MediaProvider, RenderOptions,
    StaticMediaProvider, Video, cli,
};
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConfig};
use one_shot::{OneShot, OneShotMedia};

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let media = OneShotMedia::prepare()?;
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dynamic_media");
    if !folder.join("soundtrack.wav").is_file() {
        return Err(format!(
            "Missing {}. Run examples/one-shot/tools/fetch.sh to download the soundtrack and sound effects.",
            folder.join("soundtrack.wav").display()
        )
        .into());
    }
    // Stereo music and effects are loaded at runtime; media/ is compiled in.
    let directory = MediaDirectory::read_folder(folder)?;
    let dynamic = directory.process_media_source()?;
    let all = CombinedMediaProvider::from([&media as &dyn MediaProvider, &dynamic]);
    let video = OneShot::new();

    #[cfg(target_os = "macos")]
    let gpu = fframes_skia_renderer::metal::SkiaMetalCtx::new(OneShot::WIDTH, OneShot::HEIGHT)?;
    #[cfg(not(target_os = "macos"))]
    let gpu = fframes_skia_renderer::vulkan::SkiaVulkanCtx::new(OneShot::WIDTH, OneShot::HEIGHT)?;
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
            ..Default::default()
        },
    )
    .backend(backend)
    .preview(fframes_native_player::cli_preview)
    .default_output(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("output/one-shot.mp4"))
    .run())
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("one-shot: {error}");
            ExitCode::FAILURE
        }
    }
}
