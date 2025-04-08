use fframes::StaticMediaProvider;
pub use fframes_renderer::{RenderOptions, fframes_logger, render};
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
            encoder_options: fframes_renderer::EncoderOptions {
                preferred_video_codec: Some("libx265"),
                codec_params: Some(&[("log-level", "none")]),
                sample_rate: 44100,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
