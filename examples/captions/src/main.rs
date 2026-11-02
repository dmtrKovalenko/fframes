use captions_example::{CaptionMedia, CaptionVideo};
use fframes::{RenderOptions, cli};
use std::process::ExitCode;

fn main() -> ExitCode {
    let media = match CaptionMedia::new() {
        Ok(media) => media,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let video = CaptionVideo::default();
    let runner = cli::new(
        &video,
        RenderOptions {
            media: Some(&media),
            ..Default::default()
        },
    );
    #[cfg(feature = "skia")]
    let runner = runner.renderers(fframes_skia_renderer::cli::SkiaRenderers::default());
    runner.run()
}
