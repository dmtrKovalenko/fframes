use crate::{AudioMap, Svgr};
use std::{fmt::Debug, sync::Arc};

pub enum Overlap {
    Previous(f32),
    Next(f32),
    PreviousAndNext { previous: f32, next: f32 },
    None,
}

pub fn seconds_to_frames(seconds: &f32, fps: usize) -> usize {
    (seconds * fps as f32) as usize
}

impl Overlap {
    pub(crate) fn to_frames(&self, fps: usize) -> (usize, usize) {
        match self {
            Overlap::Previous(sec) => (seconds_to_frames(sec, fps), 0),
            Overlap::Next(sec) => (0, seconds_to_frames(sec, fps)),
            Overlap::PreviousAndNext { previous, next } => (
                seconds_to_frames(previous, fps),
                seconds_to_frames(next, fps),
            ),
            Overlap::None => (0, 0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
/// Represents current scene position and duration within a video.
pub struct SceneInfo {
    /// Resolved duration of scene in frames. The `frame.index` is always < `frame.scene_info.duration_in_frames`
    pub duration_in_frames: usize,
    /// The index of the scene in a video
    pub index: usize,
    /// The total amount of scenes in a video
    pub total_scenes_in_video: usize,
    /// If `true` then this scene is defined last in the video.
    pub is_last: bool,
}

#[allow(unused_variables)]
pub trait Scene: Debug + Sync + Send {
    fn duration(&self) -> crate::Duration;
    fn render_frame(&self, frame: crate::frame::Frame, ctx: &crate::FFramesContext) -> Svgr;

    fn overlap(&self) -> Overlap {
        Overlap::None
    }

    fn audio_map(&self) -> crate::audio_map::AudioMap {
        crate::audio_map::AudioMap::none()
    }

    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

pub struct Scenes(pub(crate) Option<Vec<Arc<dyn Scene>>>);

impl Scenes {
    pub fn len(&self) -> usize {
        self.0.as_ref().map(|s| s.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl From<Vec<Arc<dyn Scene>>> for Scenes {
    fn from(arr: Vec<Arc<dyn Scene>>) -> Self {
        Self(Some(arr))
    }
}

pub struct SceneWithAudio<'a> {
    pub audio_map: AudioMap<'a>,
    pub scene: &'a Arc<dyn Scene>,
}

pub struct ScenesWithAudio<'a>(pub(crate) Option<Vec<SceneWithAudio<'a>>>);

impl ScenesWithAudio<'_> {
    pub fn len(&self) -> usize {
        self.0.as_ref().map(|s| s.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ScenesWithAudio<'_> {
    pub fn used_audio_files(&self) -> Option<Vec<&str>> {
        self.0.as_ref().map(|s| {
            s.iter()
                .flat_map(|s| s.audio_map.used_audio_files())
                .flatten()
                .collect::<Vec<_>>()
        })
    }
}

impl<'a> From<&'a Scenes> for ScenesWithAudio<'a> {
    fn from(scenes: &'a Scenes) -> Self {
        Self(scenes.0.as_ref().map(|s| {
            s.iter()
                .map(|s| SceneWithAudio {
                    audio_map: s.audio_map(),
                    scene: s,
                })
                .collect::<Vec<_>>()
        }))
    }
}
