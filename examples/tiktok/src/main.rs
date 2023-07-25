pub use fframes_renderer::{fframes_logger, render, RenderOptions};
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
            render_backend: fframes_renderer::cpu::CpuRenderingBackend {
                cache_capacity: 10,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
