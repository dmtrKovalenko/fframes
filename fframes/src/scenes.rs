use std::fmt::Debug;

use crate::Svgr;

pub enum Overlap {
    Previous(f64),
    Next(f64),
    PreviousAndNext((f64, f64)),
    None,
}

pub fn seconds_to_frames(seconds: &f64, fps: usize) -> usize {
    (seconds * fps as f64) as usize
}

impl Overlap {
    pub(crate) fn to_frames(&self, fps: usize) -> (usize, usize) {
        match self {
            Overlap::Previous(sec) => (seconds_to_frames(sec, fps), 0),
            Overlap::Next(sec) => (0, seconds_to_frames(sec, fps)),
            Overlap::PreviousAndNext((prev, next)) => {
                (seconds_to_frames(prev, fps), seconds_to_frames(next, fps))
            }
            Overlap::None => (0, 0),
        }
    }
}

pub trait Scene: Debug + Sync + Send {
    fn duration(&self) -> crate::video::Duration;
    fn render_frame(&self, frame: crate::frame::Frame, ctx: &crate::FFramesContext) -> Svgr;

    fn overlap(&self) -> Overlap {
        Overlap::None
    }

    fn audio_map(&self) -> crate::audio_map::AudioMap {
        crate::audio_map::AudioMap::none()
    }
}

pub struct Scenes(pub(crate) Option<Vec<Box<dyn Scene>>>);

impl From<Vec<Box<dyn Scene>>> for Scenes {
    fn from(arr: Vec<Box<dyn Scene>>) -> Self {
        Self(Some(arr))
    }
}
