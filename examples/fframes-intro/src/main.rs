use fframes::{
    AudioMixOptions, LimiterOptions, CombinedMediaProvider, EncoderOptions, MediaDirectory, MediaProvider, RenderOptions,
    StaticMediaProvider, cli,
};
use fframes_intro::{HEIGHT, IntroMedia, IntroVideo, WIDTH};
use fframes_skia_renderer::{
    SkiaFFramesRenderer, SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig, metal::SkiaMetalCtx,
};
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let media = IntroMedia::prepare().expect("media");
    // music, sound effects and the podcast clips: loaded at runtime, stereo
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("dynamic_media");
    let dir = MediaDirectory::read_folder(&folder).expect("dynamic media folder");
    let dynamic = dir.process_media_source().expect("dynamic media");
    let all = CombinedMediaProvider::from([&media as &dyn MediaProvider, &dynamic]);
    let gpu = SkiaMetalCtx::new(WIDTH, HEIGHT).expect("GPU context");

    cli::new(
        &IntroVideo,
        RenderOptions {
            media: Some(&all),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx264"),
                codec_params: Some(&[("crf", "16"), ("preset", "medium"), ("tune", "film")]),
                ..Default::default()
            },
            // the music edit measures about -17.5 LUFS; lift it to about -14 for the web
            audio_mix: AudioMixOptions {
                master_gain_db: 3.5,
                // room for inter-sample peaks: true peak stays under -1 dBTP
                limiter: Some(LimiterOptions { ceiling_db: -1.3, ..Default::default() }),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .backend(
        SkiaFFramesRenderer::new_metal(
            &gpu,
            SkiaPipelineConfig {
                concurrency_policy: SkiaPipelineConcurrencyPolicy::MaxPerformance,
                ..Default::default()
            },
        )
        .expect("skia renderer"),
    )
    .preview(fframes_native_player::cli_preview)
    .run()
}
