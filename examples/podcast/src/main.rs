use fframes_renderer::render_backend::CpuRenderingBackend;
pub use fframes_renderer::{fframes_logger, render, render_backend, RenderOptions};
use podcast_example::PodcastVideo;

fn main() {
    render(
        PodcastVideo {
            goose_audio: "final.mp3",
            duck_audio: "final.mp3",
            guest_audio: "final.mp3",
        },
        "out.mp4",
        RenderOptions {
            media_dir: "./media",
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: CpuRenderingBackend {
                cache_capacity: 20,
                ..Default::default()
            },
            preferred_codec: "libx264",
            ..Default::default()
        },
    )
    .unwrap();
}
