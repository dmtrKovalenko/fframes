use clap::Parser;
use fframes::{CombinedMediaProvider, MediaProvider, StaticMediaProvider};
use fframes_renderer::{EncoderOptions, MediaDirectory, RenderOptions, fframes_logger, render};
use hello_world_example::{PixelMedia, PixelVideo, RandomPhotos};
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[clap(short, long)]
    gpu: bool,
    #[clap(short, long, default_value = "out.mp4")]
    output: String,
    #[clap(long, default_value = "libx264")]
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
    let photos = RandomPhotos::new_from_media_provider(rng, &photos_media);

    render(
        "out.mp4",
        &PixelVideo::new_random_scenes("The Farewell.mp3", rng, Some(&photos_media), photos),
        fframes_renderer::cpu::CpuRenderingBackend {
            cache_capacity: 100,
            ..Default::default()
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
                preferred_audio_codec: args.audio_codec.as_deref(),
                preferred_video_codec: args.video_codec.as_deref(),
                tmp_files_directory: Some(&PathBuf::from("test_render")),
                codec_params: (args.video_codec.as_deref() == Some("libx264")
                    || args.video_codec.as_deref() == Some("libx265"))
                .then_some(&[
                    ("crf", "23"),
                    ("preset", "ultrafast"),
                    ("tune", "animation"),
                ]),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
