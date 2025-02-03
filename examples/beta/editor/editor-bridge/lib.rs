#![cfg(target_arch = "wasm32")]
use beta_example::{BetaExamples, BetaVideo, IphoneScene};
use fframes_editor_controller::{prelude::*, setup_wasm_editor};
use hello_world_example::{HelloWorldMedia, HelloWorldVideo};
use marketing_example::{MarketingMedia, MarketingVideo};
use podcast_example::PodcastVideo;
use std::sync::Arc;
use tiktok_example::{GooseMedia, GooseVideo};

lazy_static! {
    static ref TIKTOK_MEDIA: GooseMedia = GooseMedia::prepare().unwrap();
    static ref MARKETING_MEDIA: MarketingMedia = MarketingMedia::prepare().unwrap();
    static ref HELLO_WORLD_MEDIA: HelloWorldMedia = HelloWorldMedia::prepare().unwrap();
}

setup_wasm_editor!(
    BetaVideo,
    BetaVideo {
        iphone_scene: IphoneScene {
            hours: 14,
            minutes: 4,
        },
        beta_examples: BetaExamples {
            tiktok_video: Arc::new(GooseVideo {
                media: &TIKTOK_MEDIA,
            }),
            hello_world_video: Arc::new(HelloWorldVideo {
                slug: "Hello, Beta!",
                media: &HELLO_WORLD_MEDIA
            }),
            marketing_video: Arc::new(MarketingVideo {
                audio_track: "beta.mp3",
                media: &MARKETING_MEDIA,
            }),
            podcast_video: Arc::new(PodcastVideo {
                goose_audio: "beta.mp3",
                duck_audio: "beta.mp3",
                guest_audio: "beta.mp3"
            })
        }
    }
);
