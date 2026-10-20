//! The scene mirrored in its [`crate::Reflector`]. The surfaces on the reflector's plane
//! sample it, blurred by their roughness.

use glam::{Vec2, Vec4};

use super::post::Image;

/// The largest number of levels. The last level is 1/32 of the size of the first.
pub(crate) const MAX_LEVELS: usize = 6;

/// The number of levels of an image of this size. The size halves from level to level
/// while both sides are larger than 8 pixels.
pub(crate) fn level_count(width: usize, height: usize) -> usize {
    let (mut w, mut h, mut levels) = (width, height, 1);
    while levels < MAX_LEVELS && w > 8 && h > 8 {
        w = (w / 2).max(1);
        h = (h / 2).max(1);
        levels += 1;
    }
    levels
}

/// The level for a roughness: the sharpest level for mirrors, the last level for rough
/// surfaces.
pub(crate) fn level_of(roughness: f32, levels: usize) -> f32 {
    let last = (levels - 1) as f32;
    (roughness.clamp(0.0, 1.0).sqrt() * last).min(last)
}

/// The mirrored frame and its blurred levels.
pub(crate) struct ReflectionMap {
    levels: Vec<Image>,
}

impl ReflectionMap {
    /// Makes the levels from the mirrored frame: premultiplied color, coverage in `w`.
    pub(crate) fn new(width: usize, height: usize, data: Vec<Vec4>) -> Self {
        let frame = Image {
            width,
            height,
            data,
        };
        Self {
            levels: frame.mip_chain(),
        }
    }

    /// Premultiplied color and coverage at `uv` (`0..1` over the frame), blurred for
    /// `roughness`.
    pub(crate) fn sample(&self, uv: Vec2, roughness: f32) -> Vec4 {
        let lod = level_of(roughness, self.levels.len());
        let l0 = lod.floor() as usize;
        let l1 = (l0 + 1).min(self.levels.len() - 1);
        let a = self.levels[l0].sample(uv.x, uv.y);
        if l1 == l0 {
            return a;
        }
        a.lerp(self.levels[l1].sample(uv.x, uv.y), lod - l0 as f32)
    }
}
