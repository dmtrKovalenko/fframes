use fframes::StaticMediaProvider;
pub use fframes_renderer::{fframes_logger, render, RenderOptions};
use marketing_example::{MarketingMedia, MarketingVideo};

fn main() {
    let media = MarketingMedia::prepare().unwrap();

    render(
        "out.mp4",
        &MarketingVideo {
            audio_track: "marketing.mp3",
            media: &media,
        },
        fframes_renderer::cpu::CpuRenderingBackend {
            ..Default::default()
        },
        &RenderOptions {
            media: Some(&media),
            load_system_fonts: true,
            logger: fframes_logger::FFramesLoggerVariant::Compact,

            ..Default::default()
        },
    )
    .unwrap();
}
