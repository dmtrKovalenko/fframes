pub use fframes_renderer::{fframes_logger, render, render_backend, RenderOptions};
use low_poly_art_example::LowPolyVideo;

fn main() {
    render(
        LowPolyVideo {},
        "out.mp4",
        RenderOptions {
            media_dir: "./media",
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: render_backend::CpuRenderingBackend {
                // Not that big cache capacity because the whole sub-svg will be cached
                cache_capacity: 5 
            },
            preferred_codec: "libx264",
            ..Default::default()
        },
    )
    .unwrap();
}
