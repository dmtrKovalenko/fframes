use std::process::ExitCode;

use fframes::{
    AudioData, AudioMixOptions, CombinedMediaProvider, DynamicMediaProvider, EncoderOptions,
    LimiterOptions, MediaProvider, RenderOptions, cli,
};
use fframes_skia_renderer::cli::SkiaRenderers;
use lottie::{LaunchMedia, LaunchVideo, soundtrack};

fn main() -> ExitCode {
    let media = LaunchMedia::new().expect("media");
    let music = DynamicMediaProvider::default()
        .with_audio("music", AudioData::Preloaded(soundtrack::music()));
    let all = CombinedMediaProvider::from([&media as &dyn MediaProvider, &music]);

    cli::new(
        &LaunchVideo,
        RenderOptions {
            media: Some(&all),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx264"),
                codec_params: Some(&[("crf", "17"), ("preset", "medium"), ("tune", "animation")]),
                ..Default::default()
            },
            // The synthesized mix measures about -17.3 LUFS. This lifts it to about -14 for
            // social feeds, and the limiter keeps the true peak below -1 dBTP.
            audio_mix: AudioMixOptions {
                master_gain_db: 3.5,
                limiter: Some(LimiterOptions {
                    ceiling_db: -1.3,
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .renderers(SkiaRenderers::default())
    .run()
}
