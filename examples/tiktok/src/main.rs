pub use fframes_renderer::{fframes_logger, render, render_backend, RenderOptions};
use tiktok_example::GooseVideo;

fn main() {
    render(
        GooseVideo {
            audio_track: "thought.mp3",
        },
        "out.mp4",
        RenderOptions {
            media_dir: "./media",
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: render_backend::CpuRenderingBackend {
                cache_capacity: 10,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
