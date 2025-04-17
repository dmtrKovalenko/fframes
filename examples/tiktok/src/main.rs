use fframes::{RenderOptions, StaticMediaProvider, fframes_logger, render};
use tiktok_example::{GooseMedia, GooseVideo};

fn main() {
    let media = GooseMedia::prepare().unwrap();

    render(
        "out.mp4",
        &GooseVideo { media: &media },
        fframes::cpu::CpuRenderingBackend {
            cache_capacity: 20,
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
