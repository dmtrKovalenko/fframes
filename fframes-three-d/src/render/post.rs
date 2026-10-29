//! Post processing of the HDR frame: bloom, depth of field, tone mapping and the conversion
//! to premultiplied sRGB bytes.

use std::sync::LazyLock;

use glam::{Mat3, Vec2, Vec3, Vec4};
use rayon::prelude::*;

use super::reflection::{MAX_LEVELS, level_count};
use crate::color::linear_to_srgb;

/// Glow around bright pixels, like the three.js `UnrealBloomPass`.
///
/// Give it to [`Renderer::bloom`](super::Renderer::bloom). Only pixels brighter than
/// `threshold` glow. Thus the parts that must glow need an emissive intensity above it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bloom {
    /// How much glow is added.
    pub strength: f32,
    /// How far the glow spreads, `0..1`.
    pub radius: f32,
    /// The linear brightness above which pixels glow. Lit surfaces are seldom brighter
    /// than 1. Emissive parts and sparks must be brighter.
    pub threshold: f32,
}

impl Default for Bloom {
    fn default() -> Self {
        Self {
            strength: 0.8,
            radius: 0.6,
            threshold: 1.0,
        }
    }
}

/// How HDR colors map to the display, as in three.js.
///
/// Set it with [`Renderer::tone_mapping`](super::Renderer::tone_mapping).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToneMapping {
    /// Clamps each channel to `0..1`.
    Linear,
    /// three.js `ACESFilmicToneMapping`. It gives high contrast, and bright colors become
    /// white.
    #[default]
    Aces,
    /// three.js `NeutralToneMapping` (Khronos PBR Neutral). It keeps base colors accurate.
    Neutral,
}

/// An image of linear premultiplied colors, row major. The bloom and the reflection filter
/// it.
pub(crate) struct Image {
    pub width: usize,
    pub height: usize,
    pub data: Vec<Vec4>,
}

impl Image {
    /// The bilinear sample at `u`, `v` (`0..1` over the image). Outside the image it
    /// clamps to the edge, like the GPU sampler.
    pub fn sample(&self, u: f32, v: f32) -> Vec4 {
        let x = (u * self.width as f32 - 0.5).clamp(0.0, self.width as f32 - 1.0);
        let y = (v * self.height as f32 - 0.5).clamp(0.0, self.height as f32 - 1.0);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let at = |x: usize, y: usize| self.data[y * self.width + x];
        at(x0, y0)
            .lerp(at(x1, y0), fx)
            .lerp(at(x0, y1).lerp(at(x1, y1), fx), fy)
    }

    /// The image and its [`level_count`] levels. Each level has half the size of the
    /// level before it.
    pub fn mip_chain(self) -> Vec<Image> {
        let count = level_count(self.width, self.height);
        let mut levels = vec![self];
        while levels.len() < count {
            levels.push(levels[levels.len() - 1].downsample());
        }
        levels
    }

    /// Makes a copy at half the size from four bilinear taps one texel out (a 4x4 tent
    /// filter). The GPU does the same in `fs_down` and `fs_down_rgba`.
    fn downsample(&self) -> Image {
        let width = (self.width / 2).max(1);
        let height = (self.height / 2).max(1);
        let (du, dv) = (1.0 / self.width as f32, 1.0 / self.height as f32);
        let mut data = vec![Vec4::ZERO; width * height];
        data.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
            let v = (y as f32 + 0.5) / height as f32;
            for (x, out) in row.iter_mut().enumerate() {
                let u = (x as f32 + 0.5) / width as f32;
                *out = (self.sample(u - du, v - dv)
                    + self.sample(u + du, v - dv)
                    + self.sample(u - du, v + dv)
                    + self.sample(u + du, v + dv))
                    * 0.25;
            }
        });
        Image {
            width,
            height,
            data,
        }
    }

    /// Adds `coarse`, upsampled with a 3x3 tent filter, times `weight`.
    fn add_upsampled(&mut self, coarse: &Image, weight: f32) {
        let (width, height) = (self.width, self.height);
        let (du, dv) = (1.0 / coarse.width as f32, 1.0 / coarse.height as f32);
        self.data
            .par_chunks_mut(width)
            .enumerate()
            .for_each(|(y, row)| {
                let v = (y as f32 + 0.5) / height as f32;
                for (x, out) in row.iter_mut().enumerate() {
                    let u = (x as f32 + 0.5) / width as f32;
                    let mut sum = coarse.sample(u, v) * 4.0;
                    sum += (coarse.sample(u - du, v)
                        + coarse.sample(u + du, v)
                        + coarse.sample(u, v - dv)
                        + coarse.sample(u, v + dv))
                        * 2.0;
                    sum += coarse.sample(u - du, v - dv)
                        + coarse.sample(u + du, v - dv)
                        + coarse.sample(u - du, v + dv)
                        + coarse.sample(u + du, v + dv);
                    *out += sum * (weight / 16.0);
                }
            });
    }
}

/// Adds the glow of `bloom` to `hdr`, a frame of `width` x `height` pixels.
pub(crate) fn bloom(hdr: &mut [Vec4], width: usize, height: usize, bloom: &Bloom) {
    if bloom.strength <= 0.0 {
        return;
    }
    // The bright pass, at half resolution and with a soft knee.
    let (hw, hh) = ((width / 2).max(1), (height / 2).max(1));
    let threshold = bloom.threshold.max(0.0);
    let knee = threshold * 0.5 + 1e-4;
    let mut bright = vec![Vec4::ZERO; hw * hh];
    let source = &*hdr;
    bright.par_chunks_mut(hw).enumerate().for_each(|(y, row)| {
        for (x, out) in row.iter_mut().enumerate() {
            let mut sum = Vec3::ZERO;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let sx = (x * 2 + dx).min(width - 1);
                let sy = (y * 2 + dy).min(height - 1);
                sum += source[sy * width + sx].truncate();
            }
            let c = sum * 0.25;
            let brightness = c.max_element();
            let soft = (brightness - threshold + knee).clamp(0.0, 2.0 * knee);
            let soft = soft * soft / (4.0 * knee);
            let contribution = soft.max(brightness - threshold) / brightness.max(1e-4);
            *out = (c * contribution.max(0.0)).extend(0.0);
        }
    });

    // Each level glows wider than the level before it. These are the weights of the levels
    // after the first. `radius` moves weight to the wide levels.
    const FACTORS: [f32; MAX_LEVELS - 1] = [0.8, 0.6, 0.4, 0.2, 0.1];
    let mut levels = Image {
        width: hw,
        height: hh,
        data: bright,
    }
    .mip_chain();
    let Some(mut glow) = levels.pop() else {
        return;
    };
    while let Some(mut finer) = levels.pop() {
        // three.js `lerpBloomFactor`
        let f = FACTORS[levels.len()];
        finer.add_upsampled(&glow, (f + (1.2 - f - f) * bloom.radius).max(0.0));
        glow = finer;
    }
    hdr.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
        let v = (y as f32 + 0.5) / height as f32;
        for (x, px) in row.iter_mut().enumerate() {
            let u = (x as f32 + 0.5) / width as f32;
            *px += glow.sample(u, v) * bloom.strength;
        }
    });
}

fn aces(color: Vec3) -> Vec3 {
    const INPUT: Mat3 = Mat3::from_cols(
        Vec3::new(0.59719, 0.07600, 0.02840),
        Vec3::new(0.35458, 0.90834, 0.13383),
        Vec3::new(0.04823, 0.01566, 0.83777),
    );
    const OUTPUT: Mat3 = Mat3::from_cols(
        Vec3::new(1.60475, -0.10208, -0.00327),
        Vec3::new(-0.53108, 1.10813, -0.07276),
        Vec3::new(-0.07367, -0.00605, 1.07602),
    );
    let v = INPUT * (color / 0.6);
    let a = v * (v + 0.024_578_6) - 0.000_090_537;
    let b = v * (0.983_729 * v + 0.432_951) + 0.238_081;
    (OUTPUT * (a / b)).clamp(Vec3::ZERO, Vec3::ONE)
}

fn neutral(mut color: Vec3) -> Vec3 {
    const START: f32 = 0.8 - 0.04;
    const DESATURATION: f32 = 0.15;
    let x = color.min_element();
    let offset = if x < 0.08 { x - 6.25 * x * x } else { 0.04 };
    color -= offset;
    let peak = color.max_element();
    if peak < START {
        return color.max(Vec3::ZERO);
    }
    let d = 1.0 - START;
    let new_peak = 1.0 - d * d / (peak + d - START);
    color *= new_peak / peak;
    let g = 1.0 - 1.0 / (DESATURATION * (peak - new_peak) + 1.0);
    color.lerp(Vec3::splat(new_peak), g).max(Vec3::ZERO)
}

const LUT_SIZE: usize = 16384;

/// The sRGB bytes of `LUT_SIZE` linear values in `0..1`.
static SRGB: LazyLock<Vec<u8>> = LazyLock::new(|| {
    (0..LUT_SIZE)
        .map(|i| {
            (linear_to_srgb(i as f32 / (LUT_SIZE - 1) as f32) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        })
        .collect()
});

/// Tone maps `hdr` and encodes it as premultiplied sRGB RGBA8, the format of `usvgr`
/// images.
pub(crate) fn encode(
    hdr: &[Vec4],
    width: usize,
    height: usize,
    exposure: f32,
    tone_mapping: ToneMapping,
) -> Vec<u8> {
    let lut: &[u8] = &SRGB;
    let mut out = vec![0u8; width * height * 4];
    out.par_chunks_mut(width * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, out) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let px = hdr[y * width + x];
                let rgb = px.truncate().max(Vec3::ZERO) * exposure;
                // Light without coverage (glows, additive blending) becomes partly opaque light.
                let alpha = px.w.max(rgb.max_element().min(1.0)).clamp(0.0, 1.0);
                if alpha <= 0.0 {
                    continue;
                }
                let straight = rgb / alpha;
                let mapped = match tone_mapping {
                    ToneMapping::Linear => straight.clamp(Vec3::ZERO, Vec3::ONE),
                    ToneMapping::Aces => aces(straight),
                    ToneMapping::Neutral => neutral(straight).clamp(Vec3::ZERO, Vec3::ONE),
                };
                let a8 = (alpha * 255.0 + 0.5) as u8;
                let alpha = f32::from(a8) / 255.0;
                // An ordered dither against banding in dark gradients.
                let dither = (bayer(x, y) - 0.5) / 255.0;
                let channel = |c: f32| {
                    let i = ((c * (LUT_SIZE - 1) as f32) as usize).min(LUT_SIZE - 1);
                    let srgb = f32::from(lut[i]) / 255.0;
                    (((srgb + dither) * alpha).clamp(0.0, 1.0) * 255.0 + 0.5) as u8
                };
                *out = [channel(mapped.x), channel(mapped.y), channel(mapped.z), a8];
            }
        });
    out
}

/// The 4x4 Bayer threshold of a pixel, in `0..1`.
#[inline]
fn bayer(x: usize, y: usize) -> f32 {
    const M: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    (f32::from(M[y & 3][x & 3]) + 0.5) / 16.0
}

/// Blurs `hdr` by the distance from the focus, like the depth of field of a lens.
///
/// It gathers, as in the single pass bokeh of Gustafsson. Each pixel averages samples on a
/// golden angle spiral, out to the largest blur near it. A sample counts where its own blur
/// reaches back to the pixel. Samples behind a sharp pixel blur only a little into it. Thus
/// sharp edges stay sharp in front of a blurred background.
///
/// `depth` holds the depth key of each pixel: `1 / distance`, or 0 for the background.
/// `focus` is the focus distance. `blur` is the blur radius in pixels of objects very far
/// behind the focus.
pub(crate) fn depth_of_field(
    hdr: &mut [Vec4],
    depth: &[f32],
    width: usize,
    height: usize,
    focus: f32,
    blur: f32,
) {
    // Samples nearer than INNER pixels read the frame. Samples farther away read a copy at
    // half resolution: the blur is wide there, and the smaller copy stays in the cache.
    const INNER: f32 = 6.0;
    const TILE: usize = 16;
    let max_blur = (blur * 1.6).min(40.0);
    // `focus * key` is `focus / distance`.
    let coc_of = |key: f32| (blur * (1.0 - focus * key).abs()).min(max_blur);
    let coc: Vec<f32> = depth.iter().map(|&key| coc_of(key)).collect();

    // The largest blur that reaches into each tile. Pixels of tiles without blur are
    // skipped.
    let (tw, th) = (width.div_ceil(TILE), height.div_ceil(TILE));
    let mut tile_max = vec![0.0f32; tw * th];
    for y in 0..height {
        for x in 0..width {
            let t = (y / TILE) * tw + x / TILE;
            tile_max[t] = tile_max[t].max(coc[y * width + x]);
        }
    }
    let reach = (max_blur / TILE as f32).ceil() as usize;
    let mut near_max = vec![0.0f32; tw * th];
    for ty in 0..th {
        for tx in 0..tw {
            let mut m = 0.0f32;
            for ny in ty.saturating_sub(reach)..(ty + reach + 1).min(th) {
                for nx in tx.saturating_sub(reach)..(tx + reach + 1).min(tw) {
                    m = m.max(tile_max[ny * tw + nx]);
                }
            }
            near_max[ty * tw + tx] = m;
        }
    }

    let source = hdr.to_vec();
    let (hw, hh) = (width.div_ceil(2), height.div_ceil(2));
    let mut half = vec![(Vec4::ZERO, 0.0f32); hw * hh];
    half.par_chunks_mut(hw).enumerate().for_each(|(hy, row)| {
        for (hx, out) in row.iter_mut().enumerate() {
            let mut color = Vec4::ZERO;
            let mut key = 0.0;
            for dy in 0..2 {
                for dx in 0..2 {
                    let x = (hx * 2 + dx).min(width - 1);
                    let y = (hy * 2 + dy).min(height - 1);
                    color += source[y * width + x];
                    key += depth[y * width + x];
                }
            }
            *out = (color * 0.25, key * 0.25);
        }
    });
    // The sample direction turns by the golden angle from sample to sample.
    let turn = Vec2::from_angle(2.399_963);
    hdr.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
        for (x, out) in row.iter_mut().enumerate() {
            let radius_max = near_max[(y / TILE) * tw + x / TILE];
            if radius_max < 0.5 {
                continue;
            }
            let i = y * width + x;
            let center_coc = coc[i];
            let center_key = depth[i];
            let mut sum = source[i];
            let mut count = 1.0;
            let mut radius = 0.8f32;
            let mut dir = Vec2::X;
            while radius < radius_max {
                let offset = dir * radius;
                // `as usize` saturates at 0, so samples past the edge read the edge pixel.
                let (color, key) = if radius < INNER {
                    let sx = ((x as f32 + offset.x).round() as usize).min(width - 1);
                    let sy = ((y as f32 + offset.y).round() as usize).min(height - 1);
                    (source[sy * width + sx], depth[sy * width + sx])
                } else {
                    // the 2x2 block that the sample point is in
                    let sx = (((x as f32 + 0.5 + offset.x) * 0.5) as usize).min(hw - 1);
                    let sy = (((y as f32 + 0.5 + offset.y) * 0.5) as usize).min(hh - 1);
                    half[sy * hw + sx]
                };
                let mut sample_coc = coc_of(key);
                // A sample farther away than the pixel (a smaller key) blurs only a little
                // over it.
                if key < center_key {
                    sample_coc = sample_coc.min(center_coc * 2.0);
                }
                let m = (sample_coc - radius + 0.5).clamp(0.0, 1.0);
                sum += (sum / count).lerp(color, m);
                count += 1.0;
                // The far samples are 2x2 averages, so half the density is enough.
                radius += if radius < INNER { 1.6 } else { 3.2 } / radius;
                dir = turn.rotate(dir);
            }
            *out = sum / count;
        }
    });
}
