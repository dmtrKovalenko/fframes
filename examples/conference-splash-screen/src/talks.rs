//! Loads the talks of `sessions.json` and builds the video, media and render options that
//! both binaries use.
use conference_splash_screen::{ConferenceMedia, ConferenceVideo, SpeakerScene, SponsorScene};
use fframes::{
    CombinedMediaProvider, EncoderOptions, MediaDirectory, MediaProvider, RenderOptions,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::LazyLock;

#[derive(Debug, Serialize, Deserialize)]
pub struct Talk {
    #[serde(rename = "abstract")]
    pub description: String,
    pub proposal_title: String,
    pub speaker_name: String,
    pub social_links: Option<String>,
    pub avatar: Option<String>,
}

pub static TALKS: LazyLock<Vec<Talk>> =
    LazyLock::new(|| serde_json::from_str(include_str!("../sessions.json")).unwrap());

static TMP_FILES_DIRECTORY: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("./test_video"));

pub fn video<'a>(media: &'a ConferenceMedia, talk: &'a Talk) -> ConferenceVideo<'a> {
    ConferenceVideo {
        media,
        sponsor_scene: SponsorScene { media },
        speaker_scene: SpeakerScene {
            avatar: talk.avatar.as_deref(),
            media,
            speaker_name: &talk.speaker_name,
            talk_title: &talk.proposal_title,
            talk_description: &talk.description,
        },
    }
}

pub fn dynamic_media() -> MediaDirectory {
    MediaDirectory::read_folder("./dynamic_media").unwrap()
}

pub fn render_options<'a>(media: &'a CombinedMediaProvider<'a, 2>) -> RenderOptions<'a, 'a> {
    RenderOptions {
        media: Some(media as &dyn MediaProvider),
        load_system_fonts: true,
        tmp_files_directory: Some(&TMP_FILES_DIRECTORY),
        video_encoder_options: EncoderOptions {
            preferred_encoder: Some("libx265"),
            qmin: 0,
            qmax: 69,
            qcompress: 0.6,
            max_qdiff: 4,
            gop_size: 250,
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn backend() -> fframes::cpu::CpuRenderingBackend {
    fframes::cpu::CpuRenderingBackend {
        concurrency: 1,
        cache_capacity: 200,
        ..Default::default()
    }
}
