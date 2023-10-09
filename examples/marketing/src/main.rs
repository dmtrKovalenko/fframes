use fframes::{MediaProvider, StaticMediaProvider};
pub use fframes_renderer::{fframes_logger, render, RenderOptions};
use marketing_example::{MarketingMedia, MarketingVideo};

fn main() {
    let media = MarketingMedia::prepare().unwrap();

    render(
        &MarketingVideo {
            audio_track: "marketing.mp3",
            media: &media,
        },
        "out.mp4",
        RenderOptions {
            media: Some(&media),
            load_system_fonts: true,
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: fframes_renderer::cpu::CpuRenderingBackend {
                // TODO figure out caching issue with path animation
                // cache_capacity: 0,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
