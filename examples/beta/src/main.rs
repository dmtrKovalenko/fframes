use beta_example::BetaVideo;
use fframes::lazy_static::lazy_static;
use fframes::StaticMediaProvider;
use fframes_renderer::{fframes_logger, render, RenderOptions};
use hello_world_example::{HelloWorldMedia, HelloWorldVideo};
use marketing_example::{MarketingMedia, MarketingVideo};
use podcast_example::PodcastVideo;
use std::{path::Path, sync::Arc};
use tiktok_example::{GooseMedia, GooseVideo};

lazy_static! {
    static ref TIKTOK_MEDIA: GooseMedia = GooseMedia::prepare().unwrap();
    static ref MARKETING_MEDIA: MarketingMedia = MarketingMedia::prepare().unwrap();
    static ref HELLO_WORLD_MEDIA: HelloWorldMedia = HelloWorldMedia::prepare().unwrap();
}

fn main() {
    let media_folder = fframes_renderer::MediaDirectory::read_folder(Path::new("./media")).unwrap();
    let media_provider = media_folder.process_media_source().unwrap();

    render(
        &BetaVideo {
            hours: 12,
            minutes: 4,
            marketing_video: Arc::new(MarketingVideo {
                audio_track: "beta.mp3",
                media: &MARKETING_MEDIA,
            }),
            hello_world_video: Arc::new(HelloWorldVideo {
                media: &HELLO_WORLD_MEDIA,
                slug: "Hello, Beta!",
            }),
            podcast_video: Arc::new(PodcastVideo {
                duck_audio: "beta.mp3",
                goose_audio: "beta.mp3",
                guest_audio: "beta.mp3",
            }),
            tiktok_video: Arc::new(GooseVideo {
                media: &TIKTOK_MEDIA,
            }),
        },
        "out.mp4",
        RenderOptions {
            media: Some(&media_provider),
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: fframes_renderer::cpu::CpuRenderingBackend {
                cache_capacity: 30,
                ..Default::default()
            },
            default_font: "Inter",
            ..Default::default()
        },
    )
    .unwrap();
}
