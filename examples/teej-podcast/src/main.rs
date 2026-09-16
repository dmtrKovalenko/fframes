use clap::Parser;
use fframes::{EncoderOptions, MediaDirectory, RenderOptions, Video, fframes_logger};
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConfig, vulkan::SkiaVulkanCtx};
use image::{ImageBuffer, Rgba};
use std::path::Path;
use teej_podcast_example::{Chapter, TeejPodcast};

#[derive(Debug, Parser)]
struct Args {
    #[clap(short, long, default_value = "out.mp4")]
    output: String,
    #[clap(long)]
    video_codec: Option<String>,
    #[clap(long)]
    audio_codec: Option<String>,
    #[clap(short, long)]
    concurrency: Option<usize>,
    #[clap(short, long)]
    pub font: Option<String>,
    /// Render a single frame to PNG for preview instead of the full video
    #[clap(long)]
    preview: bool,
    /// Frame index to render when using --preview (default: 24, i.e. 1 second at 24fps)
    #[clap(long, default_value = "24")]
    preview_frame: usize,
}

fn main() {
    let args = Args::parse();
    let media_folder = MediaDirectory::read_folder(Path::new("./dynamic_media")).unwrap();
    let dynamic_media = media_folder.process_media_source().unwrap();
    let vulkan_ctx = SkiaVulkanCtx::new(TeejPodcast::WIDTH, TeejPodcast::HEIGHT).unwrap();

    let chapters = [
        Chapter::new("Introduction to Lunch Bites Podcast", "00:00"),
        Chapter::new("Tech Sponsorships and Streaming Quality", "01:51"),
        Chapter::new("Flexing in LA: The Casa Bonita Experience", "02:33"),
        Chapter::new("Viral Moments: The Post-It Note Debate", "03:00"),
        Chapter::new("Zuckerberg's Rebranding and Tech Culture", "04:51"),
        Chapter::new("The Intersection of Geek Culture and Popularity", "14:55"),
        Chapter::new("Psychedelic Fantasy Baseball and AI", "16:48"),
        Chapter::new("The Rise of Meme Coins", "17:52"),
        Chapter::new("Trump Coin and the Crypto Circus", "19:13"),
        Chapter::new("Fart Coin: The AI Millionaire", "21:50"),
        Chapter::new("OpenAI and the Future of AI Models", "25:46"),
        Chapter::new("Influencers and the Coding Landscape", "30:10"),
    ];

    let video = TeejPodcast::new(&chapters);

    let options = RenderOptions {
        media: Some(&dynamic_media),
        load_system_fonts: true,
        logger: fframes_logger::FFramesLoggerVariant::Compact,
        video_encoder_options: EncoderOptions {
            #[cfg(target_os = "macos")]
            preferred_encoder: Some("hevc_videotoolbox"),
            #[cfg(not(target_os = "macos"))]
            preferred_encoder: Some("libx265"),
            qmin: 0,
            qmax: 69,
            qcompress: 0.6,
            max_qdiff: 4,
            gop_size: 24,
            codec_params: Some(&[
                ("crf", "23"),
                ("preset", "slow"),
                ("tune", "film"),
                ("bframes", "3"),
            ]),
            ..Default::default()
        },
        ..Default::default()
    };

    let backend = SkiaFFramesRenderer::new_vulkan(
        &vulkan_ctx,
        SkiaPipelineConfig {
            buffer_queue_size: 20,
            concurrency_policy:
                fframes_skia_renderer::SkiaPipelineConcurrencyPolicy::MaxPerformance,
            ..Default::default()
        },
    )
    .expect("Failed to create renderer");

    if args.preview {
        let frame_buffer = fframes::render_frame(args.preview_frame, &video, backend, &options)
            .expect("Failed to render preview frame");

        let img = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(
            TeejPodcast::WIDTH as u32,
            TeejPodcast::HEIGHT as u32,
            frame_buffer,
        )
        .expect("Failed to create image buffer");

        img.save("frame_preview.png")
            .expect("Failed to save preview image");

        println!(
            "Saved frame {} preview to frame_preview.png ({}x{})",
            args.preview_frame,
            TeejPodcast::WIDTH,
            TeejPodcast::HEIGHT
        );
    } else {
        fframes::render(args.output.as_str(), &video, backend, &options).unwrap();
    }
}
