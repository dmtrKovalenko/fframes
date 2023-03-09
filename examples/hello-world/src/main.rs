pub use fframes_renderer::{fframes_logger, render, render_backend, RenderOptions};
use hello_world_example::HelloWorldVideo;

fn main() {
    render(
        HelloWorldVideo {
            slug: "Hello Renderer!",
        },
        "out.mp4",
        RenderOptions {
            media_dir: "./media",
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: render_backend::CpuRenderingBackend {
                cache_capacity: 1,
                ..Default::default()
            },
            preferred_codec: "libx264",
            ..Default::default()
        },
    )
    .unwrap();
}
