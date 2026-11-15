use fframes::{CombinedMediaProvider, MediaProvider, RenderOptions, cli};
use std::process::ExitCode;
use text_width_example::{InterMedia, TextWidthMedia, TextWidthVideo};

fn main() -> ExitCode {
    let (inter, own) = match (InterMedia::new(), TextWidthMedia::new()) {
        (Ok(inter), Ok(own)) => (inter, own),
        (Err(error), _) | (_, Err(error)) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let media = CombinedMediaProvider::from([&inter as &dyn MediaProvider, &own]);
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
