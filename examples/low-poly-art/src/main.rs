use fframes::{CombinedMediaProvider, MediaProvider, RenderOptions, fframes_logger};
use low_poly_art_example::{LowPolyMedia, LowPolyVideo, owl};

/// Make sure that this example will compile very slowly but the rendering will be (relatively) fast.
/// This is because fframes inlines and parses all the svg at compile time and skipping mostly all
/// the runtime work for processng and preparing svg.
fn main() {
    let media = LowPolyMedia::new().unwrap();
    let owl_media = owl::OwlMedia::new().unwrap();

    let combined_media = CombinedMediaProvider::from([
        &media as &dyn MediaProvider,
        &owl_media as &dyn MediaProvider,
    ]);

    fframes::render(
        "out.mp4",
        &LowPolyVideo {
            media: &media,
            scene: &owl::Owl { media: &owl_media },
        },
        fframes::cpu::CpuRenderingBackend {
            // Don't need a lot of capacity because the whole sub-svg of animal scene will be cached
            cache_capacity: 5,
            ..Default::default()
        },
        &RenderOptions {
            media: Some(&combined_media),
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            ..Default::default()
        },
    )
    .unwrap();
}
