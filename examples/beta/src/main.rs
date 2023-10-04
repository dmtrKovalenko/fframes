use std::path::Path;

use beta_example::BetaVideo;
use fframes::{DynamicMediaProvider, StaticMediaProvider};
pub use fframes_renderer::{fframes_logger, render, RenderOptions};
use marketing_example::{MarketingMedia, MarketingVideo};
use tiktok_example::GooseMedia;

fn main() {
    let tiktok_media = GooseMedia::prepare().unwrap();
    let marketing_media = MarketingMedia::prepare().unwrap();

    let media_folder = fframes_renderer::MediaDirectory::read_folder(Path::new("./media")).unwrap();
    let media_provider = media_folder.process_media_source().unwrap();

    render(
        &BetaVideo {
            hours: 12,
            minutes: 4,
            marketing_video: Arc::new(MarketingVideo {
                audio_track: "beta.mp3",
                media: &marketing_media,
            }),
            hello_world_video: todo!(),
            podcast_video: todo!(),
            tiktok_video: todo!(),
        },
        "out.mp4",
        RenderOptions {
            media: Some(&media),
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
