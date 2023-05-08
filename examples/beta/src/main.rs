use beta_example::BetaVideo;
pub use fframes_renderer::{fframes_logger, render, render_backend, RenderOptions};

fn main() {
    render(
        BetaVideo {
            hours: 12,
            minutes: 4,
        },
        "out.mp4",
        RenderOptions {
            media_dir: "./media",
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: render_backend::CpuRenderingBackend {
                cache_capacity: 30,
                ..Default::default()
            },
           
            default_font: "Inter",
            ..Default::default()
        },
    )
    .unwrap();
}
