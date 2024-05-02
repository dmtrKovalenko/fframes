use fframes::MediaProvider;
use fframes::StaticMediaProvider;
pub use fframes_renderer::{fframes_logger, render, RenderOptions};
use tiktok_example::{GooseMedia, GooseVideo};

fn main() {
    let media = GooseMedia::prepare().unwrap();
    println!("media: {:?}", media.resolve_audio("thouoght.mp3"));
    render(
        &GooseVideo { media: &media },
        "out.mp4",
        RenderOptions {
            media: Some(&media),
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
