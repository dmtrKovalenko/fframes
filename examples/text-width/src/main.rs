use fframes::{RenderOptions, cli};
use std::process::ExitCode;
use text_width_example::{TextWidthMedia, TextWidthVideo};

fn main() -> ExitCode {
    let media = match TextWidthMedia::new() {
        Ok(media) => media,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let video = TextWidthVideo;
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
