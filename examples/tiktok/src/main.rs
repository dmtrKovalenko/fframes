use fframes::StaticMediaProvider;
pub use fframes_renderer::{RenderOptions, fframes_logger, render};
use tiktok_example::{GooseMedia, GooseVideo};

fn main() {
    let media = GooseMedia::prepare().unwrap();

    render(
        "out.mp4",
        &GooseVideo { media: &media },
        fframes_renderer::cpu::CpuRenderingBackend {
            cache_capacity: 10,
            ..Default::default()
        },
        &RenderOptions {
            media: Some(&media),
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            ..Default::default()
        },
    )
    .unwrap();
}
