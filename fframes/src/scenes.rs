use crate::Svgr;
use std::fmt::Debug;

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
    fn duration(&self) -> crate::video::Duration;
    fn render_frame(&self, frame: crate::frame::Frame, ctx: &crate::FFramesContext) -> Svgr;

    fn overlap(&self) -> Overlap {
        Overlap::None
    }

    fn audio_map(&self, scene_info: &SceneInfo) -> crate::audio_map::AudioMap {
        crate::audio_map::AudioMap::none()
    }
}

pub struct Scenes(pub(crate) Option<Vec<Box<dyn Scene>>>);

impl From<Vec<Box<dyn Scene>>> for Scenes {
    fn from(arr: Vec<Box<dyn Scene>>) -> Self {
        Self(Some(arr))
    }
}
