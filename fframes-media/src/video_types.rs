use std::path::PathBuf;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq)]
pub struct ResizeVideoFrame {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq)]
pub struct FrameConvertOptions {
    pub resize: ResizeVideoFrame,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct GeneralVideoFileMetadata {
    pub width: u32,
    pub height: u32,
    pub duration: f32,
    pub fps: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoMedia {
    pub metadata: Option<GeneralVideoFileMetadata>,
    pub path: PathBuf,
}

/// Playback speed of a video source, kept to 1/1000. Speeds that round to the same
/// value are equal, so they share a decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlaybackSpeed(i32);

impl PlaybackSpeed {
    /// The denominator of `thousandths`.
    pub const SCALE: i32 = 1000;
    pub const NORMAL: Self = Self(Self::SCALE);

    /// A speed that is not positive and finite plays at normal speed.
    pub fn new(speed: f32) -> Self {
        if speed.is_finite() && speed > 0.0 {
            Self(((speed * Self::SCALE as f32).round() as i32).max(1))
        } else {
            Self::NORMAL
        }
    }

    /// The speed in 1/1000.
    pub fn thousandths(self) -> i32 {
        self.0
    }
}

impl Default for PlaybackSpeed {
    fn default() -> Self {
        Self::NORMAL
    }
}

#[cfg(test)]
mod playback_speed_tests {
    use super::PlaybackSpeed;

    #[test]
    fn speeds_are_kept_to_a_thousandth() {
        assert_eq!(PlaybackSpeed::new(1.000_099), PlaybackSpeed::NORMAL);
        assert_eq!(PlaybackSpeed::new(1.4).thousandths(), 1400);
        assert_eq!(PlaybackSpeed::new(0.0001).thousandths(), 1);
        for invalid in [0.0, -2.0, f32::NAN, f32::INFINITY] {
            assert_eq!(PlaybackSpeed::new(invalid), PlaybackSpeed::NORMAL);
        }
    }
}
