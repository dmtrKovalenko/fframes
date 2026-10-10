use fframes::cli::{self, clap};
use fframes::{
    CombinedMediaProvider, EncoderOptions, MediaDirectory, MediaProvider, RenderOptions,
    StaticMediaProvider,
};
use fframes_skia_renderer::{
    SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig, cli::SkiaRenderers,
};
use pixel_memory_example::{ALL_SONGS, PixelMedia, PixelVideo, RandomPhotos};
use rand::prelude::*;
use std::process::ExitCode;

#[derive(Debug, clap::Args)]
struct Args {
    #[arg(
        short,
        long,
        global = true,
        default_value = "They say dogs live shorter lives because they already know how to love unconditionally"
    )]
    enter_text: String,
    /// Song file. Without it the seed picks one from the collection.
    #[arg(short, long, global = true)]
    song: Option<String>,
    /// Picks the photos, scenes and song. Commands run with the same seed see the same video.
    #[arg(long, default_value_t = 48, global = true)]
    seed: u64,
}

fn main() -> ExitCode {
    let args = cli::parse::<Args>();
    let media = PixelMedia::prepare().unwrap();
    let photos = MediaDirectory::read_folder("photos").unwrap();
    let photos_media = photos.process_media_source().unwrap();
    let all_media = CombinedMediaProvider::from([
        &media as &dyn MediaProvider,
        &photos_media as &dyn MediaProvider,
    ]);

    let rng = &mut rand::rngs::StdRng::seed_from_u64(args.app.seed);
    let photos = RandomPhotos::new_from_media_provider(rng, &photos_media);
    let song = args
        .app
        .song
        .clone()
        .unwrap_or_else(|| ALL_SONGS.choose(rng).expect("no songs").to_string());
    let enter_text = args.app.enter_text.clone();
    let video = PixelVideo::new_random_scenes(&song, &enter_text, rng, Some(&media), photos);

    let options = RenderOptions {
        media: Some(&all_media),
        load_system_fonts: true,
        video_encoder_options: EncoderOptions {
            preferred_encoder: Some("libx264"),
            qmin: 0,
            qmax: 69,
            qcompress: 0.6,
            max_qdiff: 4,
            gop_size: 24,
            codec_params: Some(&[
                ("crf", "23"),
                ("preset", "slower"),
                ("profile:v", "high"),
                ("level:v", "5.1"),
                ("g", "24"),
                ("sc_threshold", "0"), // Disable scene cut detection
            ]),
            ..Default::default()
        },
        ..Default::default()
    };

    cli::new(&video, options)
        .args(args)
        .backend(fframes::cpu::CpuRenderingBackend {
            cache_capacity: 300,
            ..Default::default()
        })
        .preview(fframes_native_player::cli_preview)
        .renderers(SkiaRenderers::gpu(SkiaPipelineConfig {
            buffer_queue_size: 20,
            concurrency_policy: SkiaPipelineConcurrencyPolicy::MaxPerformance,
            ..Default::default()
        }))
        .run()
}
