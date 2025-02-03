use beta_example::{BetaExamples, BetaVideo};
use fframes::{lazy_static::lazy_static, CombinedMediaProvider, Video};
use fframes::{MediaProvider, StaticMediaProvider};
use fframes_renderer::{fframes_logger, render, EncoderOptions, RenderOptions};
use fframes_skia_renderer::vulkan::SkiaVulkanCtx;
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConfig};
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
    let fs_media = media_folder.process_media_source().unwrap();

    let media = CombinedMediaProvider::from([
        &*TIKTOK_MEDIA as &dyn MediaProvider,
        &*MARKETING_MEDIA as &dyn MediaProvider,
        &*HELLO_WORLD_MEDIA as &dyn MediaProvider,
        &fs_media as &dyn MediaProvider,
    ]);

    let vulkan_ctx = SkiaVulkanCtx::new(BetaVideo::WIDTH, BetaVideo::HEIGHT).unwrap();
    render(
        "out.mp4",
        &BetaVideo {
            iphone_scene: beta_example::IphoneScene {
                hours: 12,
                minutes: 4,
            },
            beta_examples: BetaExamples {
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
        },
        SkiaFFramesRenderer::new_vulkan(
            &vulkan_ctx,
            SkiaPipelineConfig {
                buffer_queue_size: 10,
                ..Default::default()
            },
        )
        .expect("Failed to create metal renderer"),
        &RenderOptions {
            media: Some(&media),
            load_system_fonts: true,
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            encoder_options: EncoderOptions {
                preferred_video_codec: Some("libx264"),
                ..Default::default()
            },
            default_font: "Inter",
            ..Default::default()
        },
    )
    .unwrap();
}
