use clap::Parser;
use fframes::Video;
use fframes_renderer::{fframes_logger, render, EncoderOptions, RenderOptions};
use fframes_skia_renderer::vulkan::SkiaVulkanCtx;
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConfig};
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
}

fn main() {
    let args = Args::parse();
    let media_folder =
        fframes_renderer::MediaDirectory::read_folder(Path::new("./dynamic_media")).unwrap();
    let dynamic_media = media_folder.process_media_source().unwrap();
    let vulkan_ctx = SkiaVulkanCtx::new(TeejPodcast::WIDTH, TeejPodcast::HEIGHT).unwrap();

    render(
        args.output.as_str(),
        &TeejPodcast::new(&[
            Chapter {
                title: "Introduction to Lunch Bites Podcast",
                start: "00:00",
            },
            Chapter {
                title: "Tech Sponsorships and Streaming Quality",
                start: "02:51",
            },
            Chapter {
                title: "Flexing in LA: The Casa Bonita Experience",
                start: "05:53",
            },
            Chapter {
                title: "Viral Moments: The Post-It Note Debate",
                start: "09:00",
            },
            Chapter {
                title: "Zuckerberg's Rebranding and Tech Culture",
                start: "11:59",
            },
            Chapter {
                title: "The Intersection of Geek Culture and Popularity",
                start: "14:55",
            },
            Chapter {
                title: "Psychedelic Fantasy Baseball and AI",
                start: "16:48",
            },
            Chapter {
                title: "The Rise of Meme Coins",
                start: "17:52",
            },
            Chapter {
                title: "Trump Coin and the Crypto Circus",
                start: "19:13",
            },
            Chapter {
                title: "Fart Coin: The AI Millionaire",
                start: "21:50",
            },
            Chapter {
                title: "OpenAI and the Future of AI Models",
                start: "25:46",
            },
            Chapter {
                title: "Influencers and the Coding Landscape",
                start: "30:10",
            },
        ]),
        SkiaFFramesRenderer::new_vulkan(
            &vulkan_ctx,
            SkiaPipelineConfig {
                buffer_queue_size: 20,
                concurrency_policy:
                    fframes_skia_renderer::SkiaPipelineConcurrencyPolicy::MaxPerformance,
                ..Default::default()
            },
        )
        .expect("Failed to create metal renderer"),
        &RenderOptions {
            media: Some(&dynamic_media),
            load_system_fonts: true,
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            encoder_options: EncoderOptions {
                preferred_video_codec: Some("libx264"),
                qmin: 0,
                qmax: 69,
                qcompress: 0.6,
                max_qdiff: 4,
                gop_size: 12,
                codec_params: Some(&[
                    ("crf", "27"),
                    ("preset", "slow"),
                    ("tune", "film"),
                    ("bframes", "3"),
                ]),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
