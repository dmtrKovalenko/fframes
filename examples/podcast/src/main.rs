pub use fframes_renderer::{fframes_logger, render, RenderOptions};
use podcast_example::PodcastVideo;

fn main() {
    render(
        &PodcastVideo {
            goose_audio: "final.mp3",
            duck_audio: "final.mp3",
            guest_audio: "final.mp3",
        },
        "out.mp4",
        RenderOptions {
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: fframes_renderer::cpu::CpuRenderingBackend {
                cache_capacity: 20,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
