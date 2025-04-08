use clap::Parser;
use fframes::{CombinedMediaProvider, MediaProvider, StaticMediaProvider, Video};
use fframes_renderer::{EncoderOptions, MediaDirectory, RenderOptions, fframes_logger, render};
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConfig, vulkan::SkiaVulkanCtx};
use pixel_memory_example::{PixelMedia, PixelVideo, RandomPhotos};

#[derive(Debug, Parser)]
struct Args {
    #[clap(
        short,
        long,
        default_value = "They say dogs live shorter lives because they already know how to love unconditionally"
    )]
    enter_text: String,
    #[clap(short, long, default_value = "The_Farewell.mp3")]
    song: String,
    #[clap(short, long, default_value = "out.mp4")]
    output: String,
    #[clap(long, default_value = "libx265")]
    video_codec: Option<String>,
    #[clap(long)]
    audio_codec: Option<String>,
    #[clap(short, long)]
    concurrency: Option<usize>,
    #[clap(long, default_value = "Renderer!")]
    slug: String,
    #[clap(short, long)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();
    let media = PixelMedia::prepare().unwrap();
    let photos = MediaDirectory::read_folder("photos").unwrap();
    let photos_media = photos.process_media_source().unwrap();

    let rng = &mut rand::thread_rng();
    // more randomized experience
    // let rng = &mut rand::rngs::StdRng::seed_from_u64(48);

    let photos = RandomPhotos::new_from_media_provider(rng, &photos_media);
    #[cfg(not(feature = "cpu"))]
    let vulkan_ctx = SkiaVulkanCtx::new(PixelVideo::WIDTH, PixelVideo::HEIGHT).unwrap();

    render(
        "out.mp4",
        &PixelVideo::new_random_scenes(
            &args.song,
            &args.enter_text,
            rng,
            Some(&photos_media),
            photos,
        ),
        #[cfg(feature = "cpu")]
        {
            fframes_renderer::cpu::CpuRenderingBackend {
                cache_capacity: 300,
                ..Default::default()
            }
        },
        #[cfg(not(feature = "cpu"))]
        {
            SkiaFFramesRenderer::new_vulkan(
                &vulkan_ctx,
                SkiaPipelineConfig {
                    buffer_queue_size: 20,
                    concurrency_policy:
                        fframes_skia_renderer::SkiaPipelineConcurrencyPolicy::MaxPerformance,
                    ..Default::default()
                },
            )
            .expect("Failed to create metal renderer")
        },
        &RenderOptions {
            media: Some(&CombinedMediaProvider::from([
                &media as &dyn MediaProvider,
                &photos_media as &dyn MediaProvider,
            ])),
            load_system_fonts: true,
            logger: if args.verbose {
                fframes_logger::FFramesLoggerVariant::Debug
            } else {
                fframes_logger::FFramesLoggerVariant::Compact
            },
            encoder_options: EncoderOptions {
                preferred_video_codec: Some("libx264"),
                qmin: 0,
                qmax: 69,
                qcompress: 0.6,
                max_qdiff: 4,
                gop_size: 24,
                codec_params: Some(&[
                    ("crf", "23"),
                    #[cfg(not(feature = "cpu"))]
                    ("preset", "slower"),
                    #[cfg(feature = "cpu")]
                    ("preset", "ultrafast"),
                    ("profile:v", "high"),
                    ("level:v", "5.1"),
                    ("g", "24"),
                    ("sc_threshold", "0"), // Disable scene cut detection
                ]),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
