pub use fframes_renderer::{fframes_logger, render, render_backend, RenderOptions};
use marketing_example::MarketingVideo;

fn main() {
    render(
        MarketingVideo {
            audio_track: "marketing.mp3",
        },
        "out.mp4",
        RenderOptions {
            media_dir: "./media",
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: render_backend::CpuRenderingBackend {
                cache_capacity: 20
            },
            preferred_codec: "libx264",
            ..Default::default()
        },
    )
    .unwrap();
}
