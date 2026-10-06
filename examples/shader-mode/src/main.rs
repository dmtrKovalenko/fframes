use std::process::ExitCode;

use fframes::{
    AudioMixOptions, CombinedMediaProvider, EncoderOptions, MediaDirectory, MediaProvider,
    RenderOptions, StaticMediaProvider, Video, cli,
};
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConfig};
use shader_mode::{ShaderMode, ShaderModeMedia};

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let media = ShaderModeMedia::prepare()?;
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dynamic_media");

    let directory = MediaDirectory::read_folder(folder)?;
    let dynamic = directory.process_media_source()?;
    let all = CombinedMediaProvider::from([&media as &dyn MediaProvider, &dynamic]);
    let video = ShaderMode::new();
    #[cfg(target_os = "macos")]
    let gpu =
        fframes_skia_renderer::metal::SkiaMetalCtx::new(ShaderMode::WIDTH, ShaderMode::HEIGHT)?;
    #[cfg(not(target_os = "macos"))]
    let gpu =
        fframes_skia_renderer::vulkan::SkiaVulkanCtx::new(ShaderMode::WIDTH, ShaderMode::HEIGHT)?;

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
                codec_params: Some(&[("crf", "17"), ("preset", "medium")]),
                ..Default::default()
            },
            // Play the source soundtrack in stereo, without gain or limiting.
            // Final exports copy the AAC stream directly rather than remixing it.
            audio_mix: AudioMixOptions {
                limiter: None,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .backend(backend)
    .preview(fframes_native_player::cli_preview)
    .default_output(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("output/shader-mode.mp4"))
    .run())
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("shader-mode: {error}");
            ExitCode::FAILURE
        }
    }
}
