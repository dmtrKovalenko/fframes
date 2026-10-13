//! Shadow maps of directional lights. Each is an orthographic depth map around the shadow
//! casters, read with a smooth percentage closer filter.

use glam::{Mat4, Vec3};

use super::par;
use super::prepare::{World, WorldTri};
use crate::Shadow;
use crate::material::Shading;
use crate::math::Aabb;

/// The view of the light on the box that receives shadows.
pub(crate) struct ShadowFit {
    pub view_proj: Mat4,
    /// The world size of one texel.
    pub texel_world: f32,
    /// The depth units in a world distance of one texel.
    pub depth_per_texel: f32,
}

/// An orthographic light camera around `bounds` that looks along `direction`. The map has
/// `size` x `size` texels.
pub(crate) fn fit(direction: Vec3, bounds: &Aabb, size: usize) -> ShadowFit {
    let center = bounds.center();
    let radius = (bounds.size().length() * 0.5).max(1e-3);
    let up = if direction.y.abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let view = Mat4::look_at_rh(center - direction * radius * 2.0, center, up);
    let light_bounds = bounds.transformed(&view);
    let pad = light_bounds.size().truncate().max_element() * 0.02 + 1e-3;
    let (min, max) = (light_bounds.min, light_bounds.max);
    // casters outside explicit bounds can be between the light and the box
    let near = (-max.z - radius * 4.0).max(0.0);
    let far = -min.z + pad;
    let width = (max.x - min.x) + pad * 2.0;
    let height = (max.y - min.y) + pad * 2.0;
    let extent = width.max(height);
    let (cx, cy) = (f32::midpoint(min.x, max.x), f32::midpoint(min.y, max.y));
    let projection = Mat4::orthographic_rh(
        cx - extent * 0.5,
        cx + extent * 0.5,
        cy - extent * 0.5,
        cy + extent * 0.5,
        near,
        far,
    );
    let texel_world = extent / size as f32;
    ShadowFit {
        view_proj: projection * view,
        texel_world,
        depth_per_texel: texel_world / (far - near).max(1e-6),
    }
}

/// The shadow of a directional light.
pub(crate) struct ShadowMap {
    size: usize,
    /// The nearest caster depth of each texel, row major.
    depth: Vec<f32>,
    fit: ShadowFit,
    softness: f32,
    strength: f32,
    bias: f32,
}

/// The rows of the map that one task rasterizes.
const BAND: usize = 16;

impl ShadowMap {
    /// Rasterizes the shadow casters of `world` as the light along `direction` sees them.
    /// `None` when nothing casts a shadow.
    pub(crate) fn build(direction: Vec3, settings: &Shadow, world: &World) -> Option<Self> {
        let casts = |tri: &&WorldTri| {
            let draw = &world.draws[tri.draw as usize];
            draw.cast_shadow
                && !draw.material.is_blended()
                && draw.material.shading != Shading::ShadowCatcher
        };
        let casters = || world.tris.iter().filter(casts);
        let bounds = settings.bounds.unwrap_or_else(|| {
            Aabb::from_points(
                casters().flat_map(|tri| tri.v.map(|v| world.streams.pos[v as usize])),
            )
        });
        if bounds.is_empty() {
            return None;
        }

        let size = settings.map_size.clamp(64, 8192) as usize;
        let fit = fit(direction, &bounds, size);

        // the casters in map space
        struct MapTri {
            x: [f32; 3],
            y: [f32; 3],
            z: [f32; 3],
            y0: usize,
            y1: usize,
        }
        let s = size as f32;
        let map_tris: Vec<MapTri> = casters()
            .filter_map(|tri| {
                let p = tri
                    .v
                    .map(|v| fit.view_proj.project_point3(world.streams.pos[v as usize]));
                let x = p.map(|p| (p.x * 0.5 + 0.5) * s);
                let y = p.map(|p| (0.5 - p.y * 0.5) * s);
                let z = p.map(|p| p.z.clamp(0.0, 1.0));
                let y0 = y[0].min(y[1]).min(y[2]).floor().max(0.0) as usize;
                let y1 = (y[0].max(y[1]).max(y[2]).ceil().max(0.0) as usize).min(size);
                let x0 = x[0].min(x[1]).min(x[2]);
                let x1 = x[0].max(x[1]).max(x[2]);
                (y0 < y1 && x1 >= 0.0 && x0 < s).then_some(MapTri { x, y, z, y0, y1 })
            })
            .collect();

        let bands = size.div_ceil(BAND);
        let mut bins: Vec<Vec<u32>> = vec![Vec::new(); bands];
        for (i, t) in map_tris.iter().enumerate() {
            for band in t.y0 / BAND..t.y1.div_ceil(BAND).min(bands) {
                bins[band].push(i as u32);
            }
        }

        let mut depth = vec![1.0f32; size * size];
        par::chunks_mut(&mut depth, size * BAND, |band, rows| {
            let row0 = band * BAND;
            for &ti in &bins[band] {
                let t = &map_tris[ti as usize];
                let area =
                    (t.x[1] - t.x[0]) * (t.y[2] - t.y[0]) - (t.y[1] - t.y[0]) * (t.x[2] - t.x[0]);
                if area.abs() < 1e-12 {
                    continue;
                }
                let inv = 1.0 / area;
                let edge = |i: usize, px: f32, py: f32| {
                    let (j, k) = ((i + 1) % 3, (i + 2) % 3);
                    ((t.x[k] - t.x[j]) * (py - t.y[j]) - (t.y[k] - t.y[j]) * (px - t.x[j])) * inv
                };
                let x0 = t.x[0].min(t.x[1]).min(t.x[2]).floor().max(0.0) as usize;
                let x1 = (t.x[0].max(t.x[1]).max(t.x[2]).ceil().max(0.0) as usize).min(size);
                for y in t.y0.max(row0)..t.y1.min(row0 + rows.len() / size) {
                    let py = y as f32 + 0.5;
                    let row = &mut rows[(y - row0) * size..(y - row0 + 1) * size];
                    for (x, d) in row.iter_mut().enumerate().take(x1).skip(x0) {
                        let px = x as f32 + 0.5;
                        let l0 = edge(0, px, py);
                        let l1 = edge(1, px, py);
                        let l2 = 1.0 - l0 - l1;
                        if l0 >= 0.0 && l1 >= 0.0 && l2 >= 0.0 {
                            let z = l0 * t.z[0] + l1 * t.z[1] + l2 * t.z[2];
                            if z < *d {
                                *d = z;
                            }
                        }
                    }
                }
            }
        });

        Some(Self {
            size,
            depth,
            fit,
            softness: settings.softness.max(0.0),
            strength: settings.strength.clamp(0.0, 1.0),
            bias: settings.bias,
        })
    }

    /// How much light reaches `position`: 1 when lit, `1 - strength` in full shadow. The
    /// point moves out along `normal` against shadow acne. It moves farther where the
    /// light grazes the surface (a small `n_dot_l`).
    pub(crate) fn visibility(&self, position: Vec3, normal: Vec3, n_dot_l: f32) -> f32 {
        let offset = normal * self.fit.texel_world * (1.0 + 2.0 * (1.0 - n_dot_l.clamp(0.0, 1.0)));
        let p = self.fit.view_proj.project_point3(position + offset);
        let s = self.size as f32;
        let u = (p.x * 0.5 + 0.5) * s - 0.5;
        let v = (0.5 - p.y * 0.5) * s - 0.5;
        if !(0.0..s).contains(&u) || !(0.0..s).contains(&v) || p.z > 1.0 {
            return 1.0;
        }
        let z = p.z - self.bias * self.fit.depth_per_texel;
        let r = self.softness;
        if r <= 0.0 {
            return 1.0 - self.strength * (1.0 - self.tap(u, v, z));
        }
        // The corners first: fully lit and fully shadowed areas need only these four taps.
        let corners = self.tap(u - r, v - r, z)
            + self.tap(u + r, v - r, z)
            + self.tap(u - r, v + r, z)
            + self.tap(u + r, v + r, z);
        let lit = if corners == 0.0 || corners == 4.0 {
            corners / 4.0
        } else {
            let mut sum = 0.0;
            for j in 0..4 {
                for i in 0..4 {
                    let du = (i as f32 - 1.5) / 1.5 * r;
                    let dv = (j as f32 - 1.5) / 1.5 * r;
                    sum += self.tap(u + du, v + dv, z);
                }
            }
            sum / 16.0
        };
        1.0 - self.strength * (1.0 - lit)
    }

    /// The bilinear depth comparison at the texel position `u`, `v`: 1 when `z` is lit.
    #[inline]
    fn tap(&self, u: f32, v: f32, z: f32) -> f32 {
        let max = self.size as i32 - 1;
        let x0 = u.floor();
        let y0 = v.floor();
        let (fx, fy) = (u - x0, v - y0);
        let xi = (x0 as i32).clamp(0, max) as usize;
        let yi = (y0 as i32).clamp(0, max) as usize;
        let xi1 = (x0 as i32 + 1).clamp(0, max) as usize;
        let yi1 = (y0 as i32 + 1).clamp(0, max) as usize;
        let lit = |x: usize, y: usize| {
            if z <= self.depth[y * self.size + x] {
                1.0
            } else {
                0.0
            }
        };
        let top = lit(xi, yi) * (1.0 - fx) + lit(xi1, yi) * fx;
        let bottom = lit(xi, yi1) * (1.0 - fx) + lit(xi1, yi1) * fx;
        top * (1.0 - fy) + bottom * fy
    }
}
