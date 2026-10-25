use std::process::ExitCode;

use anamorphic_video::AnamorphicVideo;
use fframes::{EncoderOptions, MediaDirectory, RenderOptions, cli};
use fframes_skia_renderer::cli::SkiaRenderers;

fn main() -> ExitCode {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("media");
    let directory = MediaDirectory::read_folder(folder).expect("media folder");
    let media = directory.process_media_source().expect("media");

    cli::new(
        &AnamorphicVideo,
        RenderOptions {
            media: Some(&media),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx264"),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .renderers(SkiaRenderers::default())
    .run()
}
