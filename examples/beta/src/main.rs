use beta_example::{BetaExamples, BetaVideo};
use fframes::{
    CombinedMediaProvider, EncoderOptions, MediaDirectory, MediaProvider, RenderOptions,
    StaticMediaProvider, cli, cpu::CpuRenderingBackend, lazy_static::lazy_static,
};
use hello_world_example::{HelloWorldMedia, HelloWorldVideo};
use marketing_example::{MarketingMedia, MarketingVideo};
use podcast_example::PodcastVideo;
use std::{path::Path, process::ExitCode, sync::Arc};
use tiktok_example::{GooseMedia, GooseVideo};

lazy_static! {
    static ref TIKTOK_MEDIA: GooseMedia = GooseMedia::prepare().unwrap();
    static ref MARKETING_MEDIA: MarketingMedia = MarketingMedia::prepare().unwrap();
    static ref HELLO_WORLD_MEDIA: HelloWorldMedia = HelloWorldMedia::prepare().unwrap();
}

fn main() -> ExitCode {
    let media_folder = MediaDirectory::read_folder(Path::new("./media")).unwrap();
    let fs_media = media_folder.process_media_source().unwrap();

    let media = CombinedMediaProvider::from([
        &*TIKTOK_MEDIA as &dyn MediaProvider,
        &*MARKETING_MEDIA as &dyn MediaProvider,
        &*HELLO_WORLD_MEDIA as &dyn MediaProvider,
        &fs_media as &dyn MediaProvider,
    ]);

    cli::new(
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
        RenderOptions {
            media: Some(&media),
            load_system_fonts: true,
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx264"),
                ..Default::default()
            },
            default_font: "Inter",
            ..Default::default()
        },
    )
    .backend(CpuRenderingBackend {
        cache_capacity: 200,
        ..Default::default()
    })
    .preview(fframes_native_player::cli_preview)
    .run()
}
