use fframes::{EncoderOptions, RenderOptions, StaticMediaProvider, cli};
use marketing_example::{MarketingMedia, MarketingVideo};
use std::process::ExitCode;

fn main() -> ExitCode {
    let media = MarketingMedia::prepare().unwrap();

    cli::new(
        &MarketingVideo {
            audio_track: "marketing.mp3",
            media: &media,
        },
        RenderOptions {
            media: Some(&media),
            load_system_fonts: true,
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx265"),
                sample_rate: 44100,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .backend(fframes::cpu::CpuRenderingBackend {
        cache_capacity: 30,
        ..Default::default()
    })
    .run()
}
