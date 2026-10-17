//! Tiled rasterization with 4x multisampling.
//!
//! The screen is split into tiles of 32x32 pixels that render in parallel. Each tile first
//! rasterizes the opaque triangles into a visibility buffer: the nearest triangle of each
//! sample. Then it shades each visible triangle once per pixel, at the centroid of the
//! samples that the triangle covers. Last, the blended triangles and particles blend into
//! the samples from back to front, with a depth test against the opaque samples.

use glam::{Mat4, Vec2, Vec3, Vec4};

use super::par::{self, SharedMut};
use super::prepare::{Draw, Rect, ScreenSprite, ScreenTri, Streams};
use super::shade::{ShadeContext, Surface, shade, uv_directions};
use crate::Blending;
use crate::scene::Background;

/// The width and height of a tile in pixels.
pub(crate) const TILE: usize = 32;
/// The triangle of a sample that no opaque triangle covers.
const NO_TRI: u32 = u32::MAX;

/// Sample offsets from the pixel center: the standard D3D 4x rotated grid.
const SAMPLES_4X: [Vec2; 4] = [
    Vec2::new(-2.0 / 16.0, -6.0 / 16.0),
    Vec2::new(6.0 / 16.0, -2.0 / 16.0),
    Vec2::new(-6.0 / 16.0, 2.0 / 16.0),
    Vec2::new(2.0 / 16.0, 6.0 / 16.0),
];
const SAMPLES_1X: [Vec2; 1] = [Vec2::ZERO];

/// A blended triangle or particle.
#[derive(Clone, Copy)]
pub(crate) enum Blended<'a> {
    Tri(&'a ScreenTri),
    Sprite(&'a ScreenSprite),
}

impl Blended<'_> {
    /// The view depth, for the order from back to front.
    pub fn depth(self) -> f32 {
        match self {
            Blended::Tri(t) => t.depth,
            Blended::Sprite(s) => s.depth,
        }
    }

    fn bounds(self) -> Rect {
        match self {
            Blended::Tri(t) => t.bounds,
            Blended::Sprite(s) => s.bounds,
        }
    }
}

/// A view that is ready to rasterize.
pub(crate) struct Frame<'a> {
    pub width: usize,
    pub height: usize,
    pub streams: &'a Streams,
    pub draws: &'a [Draw<'a>],
    pub opaque: &'a [ScreenTri],
    /// The blended triangles and particles, from back to front.
    pub blended: &'a [Blended<'a>],
    pub shade: &'a ShadeContext<'a>,
    /// Clip space to world space, for the rays of the environment background.
    pub inverse_view_proj: Mat4,
    pub multisample: bool,
}

/// Renders the frame into premultiplied linear HDR, `width * height` pixels. Also returns
/// the depth key of the nearest opaque surface of each pixel, 0 where there is none.
pub(crate) fn rasterize(frame: &Frame) -> (Vec<Vec4>, Vec<f32>) {
    let (w, h) = (frame.width, frame.height);
    let tiles_x = w.div_ceil(TILE);
    let tiles = tiles_x * h.div_ceil(TILE);
    let opaque_bins = bin(frame.opaque.iter().map(|t| t.bounds), tiles_x, tiles);
    let blended_bins = bin(frame.blended.iter().map(|b| b.bounds()), tiles_x, tiles);

    let mut out = vec![Vec4::ZERO; w * h];
    let mut depth_out = vec![0.0f32; w * h];
    let target = SharedMut::new(&mut out);
    let depth_target = SharedMut::new(&mut depth_out);
    par::for_each(tiles, |tile| {
        let (x0, y0) = (tile % tiles_x * TILE, tile / tiles_x * TILE);
        let rect = Rect {
            x0,
            y0,
            x1: (x0 + TILE).min(w),
            y1: (y0 + TILE).min(h),
        };
        render_tile(
            frame,
            &opaque_bins[tile],
            &blended_bins[tile],
            rect,
            |x, y, color, depth| {
                // SAFETY: the tiles do not overlap, so only one tile writes each pixel
                unsafe {
                    target.write(y * w + x, color);
                    depth_target.write(y * w + x, depth);
                }
            },
        );
    });
    (out, depth_out)
}

/// The indices of the items that touch each tile, from the pixel bounds of the items.
fn bin(bounds: impl Iterator<Item = Rect>, tiles_x: usize, tiles: usize) -> Vec<Vec<u32>> {
    let mut bins = vec![Vec::new(); tiles];
    for (i, r) in bounds.enumerate() {
        for ty in r.y0 / TILE..r.y1.div_ceil(TILE) {
            for tx in r.x0 / TILE..r.x1.div_ceil(TILE) {
                bins[ty * tiles_x + tx].push(i as u32);
            }
        }
    }
    bins
}

/// Whether the barycentrics `l` are in the triangle, edges included.
#[inline]
fn inside(l: Vec3) -> bool {
    l.cmpge(Vec3::ZERO).all()
}

/// Renders the pixels of `tile`. `opaque` and `blended` hold the indices of the items that
/// touch the tile. `write` stores the color and the depth key of a pixel.
fn render_tile(
    frame: &Frame,
    opaque: &[u32],
    blended: &[u32],
    tile: Rect,
    mut write: impl FnMut(usize, usize, Vec4, f32),
) {
    let samples: &[Vec2] = if frame.multisample {
        &SAMPLES_4X
    } else {
        &SAMPLES_1X
    };
    let ns = samples.len();
    let tw = tile.x1 - tile.x0;
    let th = tile.y1 - tile.y0;
    // the index of the first sample of a pixel
    let first_sample = |x: usize, y: usize| ((y - tile.y0) * tw + x - tile.x0) * ns;
    // The depth key (larger is nearer, 0 is the far plane) and the triangle of each sample.
    let mut depth = vec![0.0f32; tw * th * ns];
    let mut ids = vec![NO_TRI; tw * th * ns];

    // Visibility: the nearest opaque triangle of each sample.
    for &ti in opaque {
        let t = &frame.opaque[ti as usize];
        let r = t.bounds.intersect(tile);
        // the steps of the barycentrics and the depth key from the pixel center to the samples
        let mut dl = [Vec3::ZERO; 4];
        let mut dz = [0.0f32; 4];
        for (s, o) in samples.iter().enumerate() {
            dl[s] = t.a * o.x + t.b * o.y;
            dz[s] = t.kx * o.x + t.ky * o.y;
        }
        for y in r.y0..r.y1 {
            let cy = y as f32 + 0.5;
            for x in r.x0..r.x1 {
                let cx = x as f32 + 0.5;
                if t.clipped(cx, cy) {
                    continue;
                }
                let l = t.bary(cx, cy);
                let z = t.key(cx, cy);
                let base = first_sample(x, y);
                for s in 0..ns {
                    if inside(l + dl[s]) {
                        let zs = z + dz[s];
                        if zs > depth[base + s] {
                            depth[base + s] = zs;
                            ids[base + s] = ti;
                        }
                    }
                }
            }
        }
    }

    // Resolve: shade each triangle of a pixel once and give its color to its samples.
    let mut colors = vec![Vec4::ZERO; tw * th * ns];
    for y in tile.y0..tile.y1 {
        for x in tile.x0..tile.x1 {
            let base = first_sample(x, y);
            let px = &ids[base..base + ns];
            let mut background_color = None;
            for (s, &id) in px.iter().enumerate() {
                if id == NO_TRI {
                    colors[base + s] =
                        *background_color.get_or_insert_with(|| background(frame, x, y));
                    continue;
                }
                // an earlier sample of the same triangle shaded it already
                if px[..s].contains(&id) {
                    continue;
                }
                let mut centroid = Vec2::ZERO;
                let mut count = 0;
                for (k, &other) in px.iter().enumerate().skip(s) {
                    if other == id {
                        centroid += samples[k];
                        count += 1;
                    }
                }
                let at = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) + centroid / count as f32;
                let color = shade_tri(frame, &frame.opaque[id as usize], at);
                for (k, &other) in px.iter().enumerate().skip(s) {
                    if other == id {
                        colors[base + k] = color;
                    }
                }
            }
        }
    }

    // Blended triangles and particles, from back to front, blended into each sample.
    for &bi in blended {
        match frame.blended[bi as usize] {
            Blended::Tri(t) => {
                let additive = frame.draws[t.draw as usize].material.blending == Blending::Additive;
                let r = t.bounds.intersect(tile);
                for y in r.y0..r.y1 {
                    for x in r.x0..r.x1 {
                        let base = first_sample(x, y);
                        let center = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                        if t.clipped(center.x, center.y) {
                            continue;
                        }
                        let mut centroid = Vec2::ZERO;
                        let mut mask = 0u8;
                        for (s, o) in samples.iter().enumerate() {
                            let q = center + *o;
                            if inside(t.bary(q.x, q.y)) && t.key(q.x, q.y) > depth[base + s] {
                                centroid += *o;
                                mask |= 1 << s;
                            }
                        }
                        if mask == 0 {
                            continue;
                        }
                        let src = shade_tri(frame, t, center + centroid / mask.count_ones() as f32);
                        for s in 0..ns {
                            if mask & (1 << s) != 0 {
                                colors[base + s] = blend(colors[base + s], src, additive);
                            }
                        }
                    }
                }
            }
            Blended::Sprite(s) => {
                let r = s.bounds.intersect(tile);
                for y in r.y0..r.y1 {
                    for x in r.x0..r.x1 {
                        let base = first_sample(x, y);
                        let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(s.center);
                        if d >= s.radius + 0.5 {
                            continue;
                        }
                        // a soft core with an antialiased edge
                        let r = s.radius;
                        let falloff = 1.0 - ((d - r * 0.3) / (r * 0.7 + 0.5)).clamp(0.0, 1.0);
                        let a = falloff * falloff;
                        let src =
                            (s.color * a).extend(if s.additive { 0.0 } else { a * s.opacity });
                        for k in 0..ns {
                            if s.z > depth[base + k] {
                                colors[base + k] = blend(colors[base + k], src, s.additive);
                            }
                        }
                    }
                }
            }
        }
    }

    let weight = 1.0 / ns as f32;
    for y in tile.y0..tile.y1 {
        for x in tile.x0..tile.x1 {
            let base = first_sample(x, y);
            let sum: Vec4 = colors[base..base + ns].iter().copied().sum();
            let nearest = depth[base..base + ns].iter().copied().fold(0.0, f32::max);
            write(x, y, sum * weight, nearest);
        }
    }
}

/// Blends `src` over `dst`. Additive light adds its color and leaves the coverage.
#[inline]
fn blend(dst: Vec4, src: Vec4, additive: bool) -> Vec4 {
    if additive {
        (dst.truncate() + src.truncate()).extend(dst.w)
    } else {
        src + dst * (1.0 - src.w)
    }
}

/// Shades the triangle `t` at the screen point `at`.
fn shade_tri(frame: &Frame, t: &ScreenTri, at: Vec2) -> Vec4 {
    let b = t.perspective_bary(at.x, at.y);
    // the next pixel right and down, for the derivatives of the normal and the UV (as the
    // GPU computes them)
    let b_right = t.perspective_bary(at.x + 1.0, at.y);
    let b_down = t.perspective_bary(at.x, at.y + 1.0);
    let s = frame.streams;
    let [i0, i1, i2] = t.v.map(|v| v as usize);
    let position = s.pos[i0] * b.x + s.pos[i1] * b.y + s.pos[i2] * b.z;
    let normal_at =
        |b: Vec3| (s.nrm[i0] * b.x + s.nrm[i1] * b.y + s.nrm[i2] * b.z).normalize_or(Vec3::Y);
    let normal = normal_at(b);
    let right = normal_at(b_right) - normal;
    let down = normal_at(b_down) - normal;
    let cross = (s.pos[i1] - s.pos[i0]).cross(s.pos[i2] - s.pos[i0]);
    let draw = &frame.draws[t.draw as usize];
    let uv_at = |b: Vec3| s.uv[i0] * b.x + s.uv[i1] * b.y + s.uv[i2] * b.z;
    let (uv, uv_dx, uv_dy) = if draw.has_uv {
        let uv = uv_at(b);
        (uv, uv_at(b_right) - uv, uv_at(b_down) - uv)
    } else {
        (Vec2::ZERO, Vec2::ZERO, Vec2::ZERO)
    };
    let (dpdu, dpdv) = if draw.has_uv
        && let Some(normal_map) = &draw.material.normal_map
    {
        // the directions on the normal map, which a negative repeat mirrors
        uv_directions(
            s.pos[i1] - s.pos[i0],
            s.pos[i2] - s.pos[i0],
            (s.uv[i1] - s.uv[i0]) * normal_map.repeat,
            (s.uv[i2] - s.uv[i0]) * normal_map.repeat,
        )
    } else {
        (Vec3::ZERO, Vec3::ZERO)
    };
    let surface = Surface {
        position,
        normal,
        face_normal: cross.normalize_or(normal),
        uv,
        color: if draw.has_color {
            s.col[i0] * b.x + s.col[i1] * b.y + s.col[i2] * b.z
        } else {
            Vec4::ONE
        },
        uv_dx,
        uv_dy,
        dpdu,
        dpdv,
        normal_variation: right.length_squared() + down.length_squared(),
        screen: Vec2::new(at.x.floor() + 0.5, at.y.floor() + 0.5),
    };
    shade(frame.shade, draw, &surface)
}

/// The background of the pixel `x`, `y`.
fn background(frame: &Frame, x: usize, y: usize) -> Vec4 {
    match frame.shade.background {
        Background::Transparent => Vec4::ZERO,
        Background::Color(c) => c.0.extend(1.0),
        Background::Environment { blur } => {
            let dir = ray(frame, x as f32 + 0.5, y as f32 + 0.5);
            frame.shade.env_radiance(dir, *blur).extend(1.0)
        }
    }
}

/// The world direction of the ray through the screen point `x`, `y`.
///
/// Do not inline it. When it is inlined, the compiler computes the ray for every pixel, also
/// for the other backgrounds. With a transparent background, the tiles then executed 17%
/// more instructions.
#[inline(never)]
fn ray(frame: &Frame, x: f32, y: f32) -> Vec3 {
    let ndc_x = x / frame.width as f32 * 2.0 - 1.0;
    let ndc_y = 1.0 - y / frame.height as f32 * 2.0;
    let inverse = frame.inverse_view_proj;
    let near = inverse.project_point3(Vec3::new(ndc_x, ndc_y, 0.0));
    let far = inverse.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
    (far - near).normalize_or(Vec3::NEG_Z)
}
