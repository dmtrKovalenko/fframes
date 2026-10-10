use fframes::{EncoderOptions, MediaDirectory, RenderOptions, cli};
use fframes_skia_renderer::{
    SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig, cli::SkiaRenderers,
};
use std::path::Path;
use std::process::ExitCode;
use teej_podcast_example::{Chapter, TeejPodcast};

fn main() -> ExitCode {
    let media_folder = MediaDirectory::read_folder(Path::new("./dynamic_media")).unwrap();
    let dynamic_media = media_folder.process_media_source().unwrap();

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

    cli::new(&video, options)
        .preview(fframes_native_player::cli_preview)
        .renderers(SkiaRenderers::gpu(SkiaPipelineConfig {
            buffer_queue_size: 20,
            concurrency_policy: SkiaPipelineConcurrencyPolicy::MaxPerformance,
            ..Default::default()
        }))
        .run()
}
