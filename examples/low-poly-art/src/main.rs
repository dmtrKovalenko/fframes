use fframes::{CombinedMediaProvider, MediaProvider, RenderOptions, cli};
use low_poly_art_example::{LowPolyMedia, LowPolyVideo, owl};
use std::process::ExitCode;

/// This example compiles slowly but renders fast: fframes parses all of its SVG at compile
/// time, so the renderer does almost no SVG work per frame.
fn main() -> ExitCode {
    let media = LowPolyMedia::new().unwrap();
    let owl_media = owl::OwlMedia::new().unwrap();

    let combined_media = CombinedMediaProvider::from([
        &media as &dyn MediaProvider,
        &owl_media as &dyn MediaProvider,
    ]);

    cli::new(
        &LowPolyVideo {
            media: &media,
            scene: &owl::Owl { media: &owl_media },
        },
        RenderOptions {
            media: Some(&combined_media),
            ..Default::default()
        },
    )
    // The whole animal scene is cached as one subtree, so a small cache is enough.
    .backend(fframes::cpu::CpuRenderingBackend {
        cache_capacity: 5,
        ..Default::default()
    })
    .run()
}
