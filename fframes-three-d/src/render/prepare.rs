//! Prepares a scene for the rasterizer. First the meshes, particles and lights go into
//! world space once per frame. Then each view projects them into screen triangles and
//! screen particles.

use glam::{Mat3, Mat4, Vec2, Vec3, Vec4};

use crate::scene::{Mesh, Object};
use crate::{Blending, Camera, Light, Material, Scene, Side};

/// A mesh instance of the frame.
pub(crate) struct Draw<'s> {
    pub material: &'s Material,
    pub cast_shadow: bool,
    pub receive_shadow: bool,
    pub has_uv: bool,
    pub has_color: bool,
}

/// The world space vertices of all visible meshes.
#[derive(Default)]
pub(crate) struct Streams {
    pub pos: Vec<Vec3>,
    pub nrm: Vec<Vec3>,
    pub uv: Vec<Vec2>,
    pub col: Vec<Vec4>,
}

impl Streams {
    /// Adds the vertex at `t` from vertex `a` to vertex `b`, and returns its index.
    fn lerp(&mut self, a: u32, b: u32, t: f32) -> u32 {
        let (a, b) = (a as usize, b as usize);
        self.pos.push(self.pos[a].lerp(self.pos[b], t));
        self.nrm.push(self.nrm[a].lerp(self.nrm[b], t));
        self.uv.push(self.uv[a].lerp(self.uv[b], t));
        self.col.push(self.col[a].lerp(self.col[b], t));
        self.pos.len() as u32 - 1
    }
}

/// A world space triangle: three indices into [`Streams`] and the index of its [`Draw`].
#[derive(Clone, Copy)]
pub(crate) struct WorldTri {
    pub v: [u32; 3],
    pub draw: u32,
}

/// The type and the world space placement of a direct light.
pub(crate) enum LightKind {
    Directional {
        /// The direction in which the light travels.
        direction: Vec3,
        /// The index of the light in [`Lights::shadow_casters`].
        shadow: Option<usize>,
    },
    Point {
        position: Vec3,
        /// The distance at which the light stops, 0 for no limit.
        distance: f32,
        decay: f32,
    },
    Spot {
        position: Vec3,
        direction: Vec3,
        distance: f32,
        decay: f32,
        /// The cosine of the angle at which the cone ends.
        cos_outer: f32,
        /// The cosine of the angle at which the soft edge of the cone starts.
        cos_inner: f32,
    },
}

/// A direct light.
pub(crate) struct LightInfo {
    /// The color times the intensity.
    pub color: Vec3,
    pub kind: LightKind,
}

/// The lights of a frame.
#[derive(Default)]
pub(crate) struct Lights {
    pub direct: Vec<LightInfo>,
    /// The sum of the ambient lights.
    pub ambient: Vec3,
    /// The sums of the sky and the ground colors of the hemisphere lights.
    pub hemisphere: Option<(Vec3, Vec3)>,
    /// The direction and the shadow settings of each directional light with a shadow.
    pub shadow_casters: Vec<(Vec3, crate::Shadow)>,
}

/// The visible meshes and particles of a frame in world space.
#[derive(Default)]
pub(crate) struct World<'s> {
    pub streams: Streams,
    pub tris: Vec<WorldTri>,
    pub draws: Vec<Draw<'s>>,
    pub sprites: Vec<WorldSprite>,
}

/// A particle in world space.
pub(crate) struct WorldSprite {
    pub position: Vec3,
    pub size: f32,
    /// The color times the opacity.
    pub color: Vec3,
    pub opacity: f32,
    pub additive: bool,
}

/// Puts the visible meshes, particles and lights of `scene` into world space.
pub(crate) fn build_world(scene: &Scene) -> (World<'_>, Lights) {
    let worlds = scene.world_matrices();
    let visible = scene.visibilities();
    let mut world = World::default();
    let mut lights = Lights::default();
    for (id, node) in scene.nodes() {
        if !visible[id.index()] {
            continue;
        }
        let m = worlds[id.index()];
        match &node.object {
            Object::Group => {}
            Object::Mesh(mesh) => push_mesh(&mut world, mesh, &m, &worlds),
            Object::Light(light) => push_light(&mut lights, light, &m),
            Object::Particles(particles) => {
                let scale = m.x_axis.truncate().length();
                for p in &particles.items {
                    if p.opacity <= 0.0 {
                        continue;
                    }
                    world.sprites.push(WorldSprite {
                        position: m.transform_point3(p.position),
                        size: p.size * scale,
                        color: p.color.0 * p.opacity,
                        opacity: p.opacity,
                        additive: particles.blending == Blending::Additive,
                    });
                }
            }
        }
    }
    (world, lights)
}

/// Adds `light` with the world matrix `m` to `lights`.
pub(crate) fn push_light(lights: &mut Lights, light: &Light, m: &Mat4) {
    let rotate = |d: Vec3| m.transform_vector3(d).normalize_or(d);
    let position = m.w_axis.truncate();
    match light {
        Light::Ambient { color, intensity } => lights.ambient += color.0 * *intensity,
        Light::Hemisphere {
            sky,
            ground,
            intensity,
        } => {
            let (s, g) = lights.hemisphere.unwrap_or((Vec3::ZERO, Vec3::ZERO));
            lights.hemisphere = Some((s + sky.0 * *intensity, g + ground.0 * *intensity));
        }
        Light::Directional(l) => {
            let direction = rotate(l.direction);
            let shadow = l.shadow.as_ref().map(|shadow| {
                lights.shadow_casters.push((direction, shadow.clone()));
                lights.shadow_casters.len() - 1
            });
            lights.direct.push(LightInfo {
                color: l.color.0 * l.intensity,
                kind: LightKind::Directional { direction, shadow },
            });
        }
        Light::Point(l) => lights.direct.push(LightInfo {
            color: l.color.0 * l.intensity,
            kind: LightKind::Point {
                position,
                distance: l.distance,
                decay: l.decay,
            },
        }),
        Light::Spot(l) => lights.direct.push(LightInfo {
            color: l.color.0 * l.intensity,
            kind: LightKind::Spot {
                position,
                direction: rotate(l.direction),
                distance: l.distance,
                decay: l.decay,
                cos_outer: l.angle.cos(),
                cos_inner: (l.angle * (1.0 - l.penumbra)).cos(),
            },
        }),
    }
}

/// Adds `mesh` with the world matrix `m`. `worlds` holds the world matrices of all nodes,
/// for the joints of skinned meshes.
fn push_mesh<'s>(world: &mut World<'s>, mesh: &'s Mesh, m: &Mat4, worlds: &[Mat4]) {
    let g = &*mesh.geometry;
    if g.indices.is_empty() || g.positions.is_empty() {
        return;
    }
    let deformed = deformed(mesh, m, worlds);
    // Every copy with its world matrix and color. A deformed mesh has no copies.
    let instances = mesh.instances.as_deref().filter(|_| deformed.is_none());
    let copies: Vec<(Mat4, Vec4)> = match instances {
        Some(instances) => (instances.transforms.iter().enumerate())
            .map(|(i, t)| (*m * *t, instances.color(i)))
            .collect(),
        None => vec![(*m, Vec4::ONE)],
    };
    let draw = world.draws.len() as u32;
    world.draws.push(Draw {
        material: &mesh.material,
        cast_shadow: mesh.cast_shadow,
        receive_shadow: mesh.receive_shadow,
        has_uv: !g.uvs.is_empty(),
        has_color: !g.colors.is_empty() || instances.is_some_and(|i| !i.colors.is_empty()),
    });

    let count = g.positions.len();
    for (m, tint) in copies {
        let base = world.streams.pos.len() as u32;
        let s = &mut world.streams;
        if let Some((pos, nrm)) = &deformed {
            s.pos.extend_from_slice(pos);
            s.nrm.extend_from_slice(nrm);
        } else {
            let normal_matrix = Mat3::from_mat4(m).inverse().transpose();
            s.pos
                .extend(g.positions.iter().map(|p| m.transform_point3(*p)));
            s.nrm.extend((0..count).map(|i| {
                (normal_matrix * g.normals.get(i).copied().unwrap_or(Vec3::Y)).normalize_or(Vec3::Y)
            }));
        }
        if g.uvs.len() == count {
            s.uv.extend_from_slice(&g.uvs);
        } else {
            s.uv.extend(std::iter::repeat_n(Vec2::ZERO, count));
        }
        if g.colors.len() == count {
            s.col.extend(g.colors.iter().map(|c| *c * tint));
        } else {
            s.col.extend(std::iter::repeat_n(tint, count));
        }

        // A mirroring world matrix turns the winding around. Skinned vertices do not use it.
        let mirrored = mesh.skin.is_none() && m.determinant() < 0.0;
        let tris = g.indices.as_chunks::<3>().0.iter();
        world.tris.extend(
            tris.filter(|t| t.iter().all(|&i| (i as usize) < count))
                .map(|t| {
                    let [a, b, c] = t.map(|i| base + i);
                    let v = if mirrored { [a, c, b] } else { [a, b, c] };
                    WorldTri { v, draw }
                }),
        );
    }
}

/// The world space positions and normals of a skinned or morphed mesh. `None` for a rigid
/// mesh, which its world matrix `m` places.
pub(crate) fn deformed(mesh: &Mesh, m: &Mat4, worlds: &[Mat4]) -> Option<(Vec<Vec3>, Vec<Vec3>)> {
    let g = &*mesh.geometry;
    let count = g.positions.len();
    let morphing = mesh.morph_weights.iter().any(|w| *w != 0.0) && !g.morph_targets.is_empty();
    let skinning = mesh.skin.is_some() && g.skin.is_some();
    if !morphing && !skinning {
        return None;
    }
    let mut pos = g.positions.clone();
    let mut nrm: Vec<Vec3> = (0..count)
        .map(|i| g.normals.get(i).copied().unwrap_or(Vec3::Y))
        .collect();
    if morphing {
        for (target, &w) in g.morph_targets.iter().zip(&mesh.morph_weights) {
            if w == 0.0 {
                continue;
            }
            for (p, d) in pos.iter_mut().zip(&target.positions) {
                *p += *d * w;
            }
            for (n, d) in nrm.iter_mut().zip(&target.normals) {
                *n += *d * w;
            }
        }
    }
    if let (Some(skin), Some(weights)) = (&mesh.skin, &g.skin) {
        let joints: Vec<Mat4> = skin
            .joints
            .iter()
            .enumerate()
            .map(|(i, joint)| {
                let world = worlds.get(joint.index()).copied().unwrap_or(Mat4::IDENTITY);
                let inverse_bind = skin.inverse_bind_matrices.get(i).copied();
                world * inverse_bind.unwrap_or(Mat4::IDENTITY)
            })
            .collect();
        for i in 0..count {
            let vertex_joints = weights.joints.get(i).copied().unwrap_or_default();
            let vertex_weights = weights.weights.get(i).copied();
            let mut skin_m = Mat4::ZERO;
            for (&joint, &w) in vertex_joints
                .iter()
                .zip(&vertex_weights.unwrap_or([1.0, 0.0, 0.0, 0.0]))
            {
                if w != 0.0 {
                    skin_m += joints
                        .get(joint as usize)
                        .copied()
                        .unwrap_or(Mat4::IDENTITY)
                        * w;
                }
            }
            pos[i] = skin_m.transform_point3(pos[i]);
            nrm[i] = (Mat3::from_mat4(skin_m) * nrm[i]).normalize_or(Vec3::Y);
        }
    } else {
        let normal_matrix = Mat3::from_mat4(*m).inverse().transpose();
        for (p, n) in pos.iter_mut().zip(&mut nrm) {
            *p = m.transform_point3(*p);
            *n = (normal_matrix * *n).normalize_or(Vec3::Y);
        }
    }
    Some((pos, nrm))
}

/// Pixel bounds, half open: `x0..x1` and `y0..y1`.
#[derive(Clone, Copy)]
pub(crate) struct Rect {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl Rect {
    /// The pixels that the box from `min` to `max` touches on a screen of `size` pixels.
    /// `None` when the box touches no pixel.
    fn covering(min: Vec2, max: Vec2, size: Vec2) -> Option<Rect> {
        let lo = min.floor().max(Vec2::ZERO);
        let hi = max.ceil().min(size);
        (lo.x < hi.x && lo.y < hi.y).then_some(Rect {
            x0: lo.x as usize,
            y0: lo.y as usize,
            x1: hi.x as usize,
            y1: hi.y as usize,
        })
    }

    /// The pixels that are in both rectangles.
    pub fn intersect(self, other: Rect) -> Rect {
        Rect {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        }
    }
}

/// A triangle in screen space, ready to rasterize.
#[derive(Clone, Copy)]
pub(crate) struct ScreenTri {
    /// The barycentric planes: the barycentrics at `x`, `y` are `a * x + b * y + c`.
    pub a: Vec3,
    pub b: Vec3,
    pub c: Vec3,
    /// The depth key plane, larger is nearer. The key is `1 / w` for perspective cameras
    /// and `1 - z` for orthographic cameras. Both are affine in screen space. The plane is
    /// relative to the first vertex at `ox`, `oy`. With absolute pixel coordinates, the
    /// constant term cancels most of the precision. Then touching surfaces flicker.
    pub kx: f32,
    pub ky: f32,
    pub k0: f32,
    pub ox: f32,
    pub oy: f32,
    pub inv_w: Vec3,
    pub v: [u32; 3],
    pub draw: u32,
    /// The clip plane distance of each vertex times its `1 / w`. At a pixel, the sum
    /// weighted by the barycentrics has the sign of the distance.
    pub clip: Option<Vec3>,
    pub bounds: Rect,
    /// The average view depth, for the order of blended triangles.
    pub depth: f32,
}

impl ScreenTri {
    #[inline]
    pub fn key(&self, x: f32, y: f32) -> f32 {
        self.k0 + self.kx * (x - self.ox) + self.ky * (y - self.oy)
    }

    #[inline]
    pub fn bary(&self, x: f32, y: f32) -> Vec3 {
        self.a * x + self.b * y + self.c
    }

    /// Whether the screen point is behind the clip plane.
    #[inline]
    pub fn clipped(&self, x: f32, y: f32) -> bool {
        self.clip.is_some_and(|e| self.bary(x, y).dot(e) < 0.0)
    }

    /// The perspective correct barycentrics at a screen point.
    #[inline]
    pub fn perspective_bary(&self, x: f32, y: f32) -> Vec3 {
        let p = self.bary(x, y) * self.inv_w;
        let sum = p.element_sum();
        if sum.abs() < 1e-20 {
            return Vec3::splat(1.0 / 3.0);
        }
        p / sum
    }
}

/// The clipping planes in clip space: `dot(plane, clip) >= 0` is inside. They are the near
/// plane, a guard band and the far plane. The guard band keeps screen coordinates in a range
/// where the `f32` edge functions stay exact. The GPU also clips at the far plane.
const GUARD: f32 = 32.0;
const PLANES: [Vec4; 6] = [
    Vec4::new(0.0, 0.0, 1.0, 0.0),
    Vec4::new(-1.0, 0.0, 0.0, GUARD),
    Vec4::new(1.0, 0.0, 0.0, GUARD),
    Vec4::new(0.0, -1.0, 0.0, GUARD),
    Vec4::new(0.0, 1.0, 0.0, GUARD),
    Vec4::new(0.0, 0.0, -1.0, 1.0),
];

/// Bit `i` is set when the clip space point `c` is outside `PLANES[i]`, with the guard
/// band scaled to `guard`. With `guard` 1 the guard planes are the sides of the frustum.
fn outcode(c: Vec4, guard: f32) -> u8 {
    u8::from(c.z < 0.0)
        | (u8::from(c.x > c.w * guard) << 1)
        | (u8::from(c.x < -c.w * guard) << 2)
        | (u8::from(c.y > c.w * guard) << 3)
        | (u8::from(c.y < -c.w * guard) << 4)
        | (u8::from(c.z > c.w) << 5)
}

/// A view of the scene.
pub(crate) struct Viewport {
    /// The size in pixels.
    pub size: Vec2,
    pub view_proj: Mat4,
    pub ortho: bool,
    /// The view is a mirror image (the reflection): front and back faces swap.
    pub mirrored: bool,
    /// Only what is in front of this plane (`xyz` normal, `w` offset) is drawn. The rest
    /// of a reflection is behind the mirror.
    pub clip_plane: Option<Vec4>,
}

/// Projects and clips the triangles of the draws that pass `filter`. Returns the screen
/// triangles of the opaque draws and of the blended draws. Back faces are culled for the
/// side of each material. Clipping adds vertices to `world.streams`.
pub(crate) fn setup_triangles(
    world: &mut World,
    viewport: &Viewport,
    filter: impl Fn(&Draw) -> bool,
) -> (Vec<ScreenTri>, Vec<ScreenTri>) {
    let streams = &mut world.streams;
    let mut clip: Vec<Vec4> = streams
        .pos
        .iter()
        .map(|p| viewport.view_proj * p.extend(1.0))
        .collect();
    let mut opaque = Vec::with_capacity(world.tris.len());
    let mut blended = Vec::new();
    for tri in &world.tris {
        let draw = &world.draws[tri.draw as usize];
        if !filter(draw) {
            continue;
        }
        let out = if draw.material.is_blended() {
            &mut blended
        } else {
            &mut opaque
        };
        let side = draw.material.side;
        let c = tri.v.map(|i| clip[i as usize]);
        // All three vertices are outside the same plane of the frustum.
        let outside = c.map(|c| outcode(c, 1.0));
        if outside[0] & outside[1] & outside[2] != 0 {
            continue;
        }
        let codes = c.map(|c| outcode(c, GUARD));
        let crossed = codes[0] | codes[1] | codes[2];
        if crossed == 0 {
            out.extend(screen_tri(tri.v, c, &streams.pos, tri.draw, side, viewport));
            continue;
        }
        // Sutherland-Hodgman against the planes that the triangle crosses
        let mut poly: Vec<u32> = tri.v.to_vec();
        for (bit, plane) in PLANES.iter().enumerate() {
            if crossed & (1 << bit) == 0 {
                continue;
            }
            let mut next = Vec::with_capacity(poly.len() + 2);
            for i in 0..poly.len() {
                let a = poly[i];
                let b = poly[(i + 1) % poly.len()];
                let da = plane.dot(clip[a as usize]);
                let db = plane.dot(clip[b as usize]);
                if da >= 0.0 {
                    next.push(a);
                }
                if (da >= 0.0) != (db >= 0.0) {
                    let t = da / (da - db);
                    next.push(streams.lerp(a, b, t));
                    clip.push(clip[a as usize].lerp(clip[b as usize], t));
                }
            }
            poly = next;
            if poly.len() < 3 {
                break;
            }
        }
        for i in 1..poly.len().saturating_sub(1) {
            let v = [poly[0], poly[i], poly[i + 1]];
            let c = v.map(|i| clip[i as usize]);
            out.extend(screen_tri(v, c, &streams.pos, tri.draw, side, viewport));
        }
    }
    (opaque, blended)
}

/// The screen triangle of the vertices `v` at the clip space positions `clip`. `None` when
/// it is culled or covers no pixel.
fn screen_tri(
    v: [u32; 3],
    clip: [Vec4; 3],
    positions: &[Vec3],
    draw: u32,
    side: Side,
    vp: &Viewport,
) -> Option<ScreenTri> {
    let inv_w = Vec3::from_array(clip.map(|c| 1.0 / c.w));
    let clip_plane = vp.clip_plane.map(|plane| {
        Vec3::from_array(
            [0, 1, 2]
                .map(|i| (plane.truncate().dot(positions[v[i] as usize]) + plane.w) * inv_w[i]),
        )
    });
    let p = [0, 1, 2].map(|i| {
        let ndc = Vec2::new(clip[i].x, clip[i].y) * inv_w[i];
        Vec2::new(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * vp.size
    });
    let key = if vp.ortho {
        Vec3::ONE - Vec3::new(clip[0].z, clip[1].z, clip[2].z) * inv_w
    } else {
        inv_w
    };
    let area2 = (p[1] - p[0]).perp_dot(p[2] - p[0]);
    if !area2.is_finite() || area2.abs() < 1e-9 {
        return None;
    }
    // Counter-clockwise in the world (y up) is clockwise on the screen (y down). A mirror
    // swaps the two.
    let back_facing = (area2 > 0.0) != vp.mirrored;
    match side {
        Side::Front if back_facing => return None,
        Side::Back if !back_facing => return None,
        _ => {}
    }
    let bounds = Rect::covering(p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]), vp.size)?;
    // Barycentric `i` is the area to the edge from vertex `i + 1` to `i + 2`, over the
    // triangle area.
    let inv_area = 1.0 / area2;
    let (mut a, mut b, mut c) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
    for i in 0..3 {
        let (j, k) = (p[(i + 1) % 3], p[(i + 2) % 3]);
        let edge = k - j;
        a[i] = -edge.y * inv_area;
        b[i] = edge.x * inv_area;
        c[i] = j.perp_dot(edge) * inv_area;
    }
    Some(ScreenTri {
        a,
        b,
        c,
        // The barycentric slopes add up to zero, so the differences to vertex 0 are enough.
        kx: a.y * (key.y - key.x) + a.z * (key.z - key.x),
        ky: b.y * (key.y - key.x) + b.z * (key.z - key.x),
        k0: key.x,
        ox: p[0].x,
        oy: p[0].y,
        inv_w,
        v,
        draw,
        clip: clip_plane,
        bounds,
        depth: (clip[0].w + clip[1].w + clip[2].w) / 3.0,
    })
}

/// A particle projected to the screen: a round blob.
pub(crate) struct ScreenSprite {
    pub center: Vec2,
    /// The depth key, larger is nearer.
    pub z: f32,
    pub radius: f32,
    /// The color times the opacity.
    pub color: Vec3,
    /// The part of the pixel that a blended sprite covers at its center.
    pub opacity: f32,
    pub additive: bool,
    pub bounds: Rect,
    /// The view depth, for the order of blended particles.
    pub depth: f32,
}

/// Projects the particles to the screen of `vp`.
pub(crate) fn setup_sprites(
    sprites: &[WorldSprite],
    camera: &Camera,
    vp: &Viewport,
) -> Vec<ScreenSprite> {
    // The screen position, the depth key and `w`. `None` in front of the near plane or
    // beyond the far plane, where the GPU clips the particle.
    let project = |p: Vec3| -> Option<(Vec2, f32, f32)> {
        let c = vp.view_proj * p.extend(1.0);
        if c.w <= camera.near || c.z > c.w {
            return None;
        }
        let screen = Vec2::new(c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5) * vp.size;
        let key = if vp.ortho { 1.0 - c.z / c.w } else { 1.0 / c.w };
        Some((screen, key, c.w))
    };
    // pixels per world unit, at a distance of 1 for perspective cameras
    let focal = match camera.ortho_height {
        Some(h) => vp.size.y / h,
        None => vp.size.y / (2.0 * (camera.fov.to_radians() * 0.5).tan()),
    };
    let project_sprite = |s: &WorldSprite| {
        let (center, z, w) = project(s.position)?;
        let radius = if camera.ortho_height.is_some() {
            s.size * focal
        } else {
            s.size * focal / w
        };
        // Sprites smaller than a pixel get a radius of 0.6 pixels and keep their energy.
        let drawn = radius.max(0.6);
        let energy = (radius / drawn).powi(2);
        let opacity = s.opacity * energy;
        if opacity <= 1e-5 {
            return None;
        }
        Some(ScreenSprite {
            center,
            z,
            radius: drawn,
            color: s.color * energy,
            opacity,
            additive: s.additive,
            bounds: Rect::covering(center - drawn, center + drawn, vp.size)?,
            depth: w,
        })
    };
    sprites.iter().filter_map(project_sprite).collect()
}
