use fframes::{EncoderOptions, RenderOptions, StaticMediaProvider, fframes_logger};
use marketing_example::{MarketingMedia, MarketingVideo};

fn main() {
    let media = MarketingMedia::prepare().unwrap();

    fframes::render(
        "out.mp4",
        &MarketingVideo {
            audio_track: "marketing.mp3",
            media: &media,
        },
        fframes::cpu::CpuRenderingBackend {
            cache_capacity: 30,
            ..Default::default()
        },
        &RenderOptions {
            media: Some(&media),
            load_system_fonts: true,
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx265"),
                sample_rate: 44100,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
