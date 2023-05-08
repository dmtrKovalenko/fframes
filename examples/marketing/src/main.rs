pub use fframes_renderer::{fframes_logger, render, render_backend, RenderOptions};
use marketing_example::MarketingVideo;

fn main() {
    render(
        MarketingVideo {
            audio_track: "marketing.mp3",
        },
        "out.mp4",
        RenderOptions {
            media_dir: "/Users/dmtrkovalenko/dev/fframes/examples/marketing/media",
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: render_backend::CpuRenderingBackend {
                // TODO figure out caching issue with path animation
                cache_capacity: 0,
                ..Default::default()
            },
           
            ..Default::default()
        },
    )
    .unwrap();
}
