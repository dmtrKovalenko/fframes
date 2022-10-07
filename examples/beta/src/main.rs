pub use fframes_renderer::{fframes_logger, render, render_backend, RenderOptions};
use beta_example::BetaVideo;

fn main() {
    render(
        BetaVideo {},
        "out.mp4",
        RenderOptions {
            media_dir: "./media",
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: render_backend::CpuRenderingBackend {},
            preferred_codec: "libx264",
            ..Default::default()
        },
    )
    .unwrap();
}
