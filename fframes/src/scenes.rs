use crate::{AudioMap, Svgr};
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
    /// The start frame index of the scene
    pub start_frame: usize,
    /// The end frame index of the scene
    pub end_frame: usize,
}

#[allow(unused_variables)]
pub trait Scene: Debug + Sync + Send {
    fn duration(&self) -> crate::Duration<'_>;
    fn render_frame<'a>(
        &'a self,
        frame: crate::Frame,
        ctx: &crate::FFramesContext<'a, '_>,
    ) -> Svgr<'a>;

    fn overlap(&self) -> Overlap {
        Overlap::None
    }

    fn audio(&self) -> crate::audio_map::AudioMap<'_> {
        crate::audio_map::AudioMap::none()
    }

    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

#[derive(Debug, Clone)]
pub struct Scenes<'a>(pub(crate) Option<Vec<&'a (dyn Scene + 'a)>>);

impl Scenes<'_> {
    pub const fn empty() -> Self {
        Self(None)
    }

    pub fn len(&self) -> usize {
        self.0.as_ref().map(|s| s.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'a> From<Vec<&'a dyn Scene>> for Scenes<'a> {
    fn from(arr: Vec<&'a dyn Scene>) -> Self {
        Self(Some(arr))
    }
}

impl<'a, T: AsRef<dyn Scene + 'a>> From<&'a [T]> for Scenes<'a> {
    fn from(arr: &'a [T]) -> Self {
        Self(Some(arr.iter().map(|s| s.as_ref()).collect()))
    }
}

#[derive(Debug)]
pub struct SceneWithAudio<'a> {
    pub audio_map: AudioMap<'a>,
    pub scene: &'a (dyn Scene + 'a),
}

#[derive(Debug)]
pub struct ScenesWithAudio<'a>(pub(crate) Option<Vec<SceneWithAudio<'a>>>);

impl<'a> ScenesWithAudio<'a> {
    pub fn new(scenes: &Scenes<'a>) -> Self {
        Self(scenes.0.as_ref().map(|s| {
            s.iter()
                .map(|s| SceneWithAudio {
                    audio_map: s.audio(),
                    scene: *s,
                })
                .collect::<Vec<_>>()
        }))
    }

    pub fn len(&self) -> usize {
        self.0.as_ref().map(|s| s.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'a> ScenesWithAudio<'a> {
    /// Audio files referenced by the scenes' audio maps and durations.
    pub fn used_audio_files(&self) -> Option<Vec<&str>> {
        self.0.as_ref().map(|scenes| {
            scenes
                .iter()
                .flat_map(|scene| {
                    let from_audio_map = scene.audio_map.used_audio_files::<Vec<&str>>();
                    let scene: &'a dyn Scene = scene.scene;
                    let from_duration = scene.duration().used_audio_files();
                    from_audio_map
                        .into_iter()
                        .flatten()
                        .chain(from_duration.into_iter().flatten())
                })
                .collect()
        })
    }

    /// Video files whose metadata the scenes' durations are resolved from.
    pub fn used_video_files(&self) -> Option<Vec<&str>> {
        self.0.as_ref().map(|scenes| {
            scenes
                .iter()
                .flat_map(|scene| {
                    let scene: &'a dyn Scene = scene.scene;
                    scene.duration().used_video_files()
                })
                .flatten()
                .collect()
        })
    }
}
