use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use glam::{IVec2, IVec3, Mat3, Mat4, Quat, Vec2, Vec3, Vec4};

use crate::Rgb;
use crate::math::Aabb;

/// Joint indices and weights of the skinned vertices, one entry per vertex. See
/// [`Geometry::skin`].
#[derive(Debug, Clone, Default)]
pub struct SkinWeights {
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
}

/// One morph target (blend shape): position and normal offsets, one per vertex.
#[derive(Debug, Clone, Default)]
pub struct MorphTarget {
    pub name: Option<String>,
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
}

/// Indexed triangles with vertex attributes, like three.js `BufferGeometry`.
///
/// Build a geometry once, in the video constructor, and share it. [`crate::Node::mesh`]
/// keeps it in an `Arc<Geometry>`, so a scene clone in each frame does not copy vertices.
///
/// A triangle is counter-clockwise when you look at the side that its normal points to.
/// All primitives have their center at the origin, as in three.js.
#[derive(Debug, Clone, Default)]
pub struct Geometry {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    /// Texture coordinates: empty, or one per vertex.
    pub uvs: Vec<Vec2>,
    /// Linear RGBA colors: empty, or one per vertex. The material color multiplies them.
    pub colors: Vec<Vec4>,
    /// Three vertex indices for each triangle.
    pub indices: Vec<u32>,
    pub skin: Option<SkinWeights>,
    pub morph_targets: Vec<MorphTarget>,
    bounds: Aabb,
}

impl Geometry {
    /// Makes a geometry from vertex buffers and triangle indices. When `normals` does not
    /// have one normal per position (for example, when it is empty), this function
    /// calculates smooth normals. Triangles with an index out of range are left out.
    pub fn new(
        positions: Vec<Vec3>,
        normals: Vec<Vec3>,
        uvs: Vec<Vec2>,
        indices: Vec<u32>,
    ) -> Self {
        let count = positions.len();
        let indices = indices
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|t| t.iter().all(|&i| (i as usize) < count))
            .flatten()
            .copied()
            .collect();
        let mut geometry = Geometry {
            positions,
            normals,
            uvs,
            indices,
            ..Default::default()
        };
        if geometry.normals.len() != geometry.positions.len() {
            geometry.compute_vertex_normals();
        }
        geometry.update_bounds();
        geometry
    }

    /// Makes a flat-shaded geometry from a triangle soup, where each three positions are
    /// one triangle. The triangles share no vertices, and all UVs are zero.
    pub fn from_triangles(positions: &[Vec3]) -> Self {
        let mut b = Builder::default();
        for &tri in positions.as_chunks::<3>().0 {
            b.flat_triangle(tri, face_normal(tri).normalize_or_zero());
        }
        b.build()
    }

    /// Loads a binary or ASCII STL model, like three.js `STLLoader`.
    ///
    /// The result is flat shaded, with one vertex for each triangle corner and no UVs. STL
    /// has only triangles. It has no materials, and its normals are not good enough to
    /// keep. The [`crate::Material`] gives the finish, and [`Geometry::smooth`] rounds the
    /// curved parts. When the winding of a triangle disagrees with its normal in the file,
    /// the normal sets the front side.
    ///
    /// STL has no unit and no up axis. Most files use millimeters with Z up.
    /// `.scale(0.001, 0.001, 0.001).rotate_x(-FRAC_PI_2)` changes them to meters with Y up.
    pub fn from_stl(bytes: &[u8]) -> Result<Self, StlError> {
        let triangles = match binary_stl(bytes) {
            Some(triangles) => triangles,
            None => ascii_stl(bytes)?,
        };
        let mut b = Builder::default();
        for (normal, [p0, p1, p2]) in triangles {
            let n = face_normal([p0, p1, p2]);
            if !n.is_finite() || n.length_squared() < 1e-24 {
                continue;
            }
            if normal.length_squared() > 1e-12 && n.dot(normal) < 0.0 {
                b.flat_triangle([p0, p2, p1], -n.normalize());
            } else {
                b.flat_triangle([p0, p1, p2], n.normalize());
            }
        }
        if b.positions.is_empty() {
            return Err(StlError::Empty);
        }
        let mut geometry = b.build();
        geometry.uvs.clear();
        Ok(geometry)
    }

    /// Welds the vertices at the same position and smooths the normals, like three.js
    /// `toCreasedNormals`. Only faces that meet at less than `crease_degrees` share a
    /// normal, so sharp edges stay sharp. Vertices with different UVs stay apart, so
    /// texture seams stay. Use it on triangle soups, for example from
    /// [`Geometry::from_stl`] or [`Geometry::flat`]. The result has no skin and no morph
    /// targets.
    pub fn smooth(self, crease_degrees: f32) -> Self {
        let cos_crease = crease_degrees.to_radians().cos();
        let triangles: Vec<[u32; 3]> = self.triangles().collect();
        // The length of a face normal is two times the face area. Large faces thus have
        // more weight in the sums below.
        let faces: Vec<Vec3> = triangles
            .iter()
            .map(|t| face_normal(t.map(|i| self.positions[i as usize])))
            .collect();
        // Positions that round to the same point on a 1e-5 grid are one position.
        let key = |p: Vec3| (p * 1e5).round().as_ivec3();
        let mut faces_at: HashMap<IVec3, Vec<usize>> = HashMap::new();
        for (f, t) in triangles.iter().enumerate() {
            for &i in t {
                faces_at
                    .entry(key(self.positions[i as usize]))
                    .or_default()
                    .push(f);
            }
        }
        // One vertex for each position, normal and UV.
        let uv_key = |i: usize| {
            self.uvs
                .get(i)
                .map_or(IVec2::ZERO, |uv| (*uv * 1e5).round().as_ivec2())
        };
        let mut welded: HashMap<(IVec3, IVec3, IVec2), u32> = HashMap::new();
        let mut b = Builder::default();
        for (t, face) in triangles.iter().zip(&faces) {
            let own = face.normalize_or(Vec3::Y);
            let [i0, i1, i2] = t.map(|i| {
                let i = i as usize;
                let position = key(self.positions[i]);
                let n = faces_at[&position]
                    .iter()
                    .map(|&f| faces[f])
                    .filter(|other| other.normalize_or(Vec3::Y).dot(own) >= cos_crease)
                    .sum::<Vec3>()
                    .normalize_or(own);
                *welded
                    .entry((position, key(n), uv_key(i)))
                    .or_insert_with(|| b.copy_vertex(&self, i, n))
            });
            b.tri(i0, i1, i2);
        }
        b.build()
    }

    /// The triangles whose indices are all in range.
    fn triangles(&self) -> impl Iterator<Item = [u32; 3]> + '_ {
        let count = self.positions.len();
        self.indices
            .as_chunks::<3>()
            .0
            .iter()
            .copied()
            .filter(move |t| t.iter().all(|&i| (i as usize) < count))
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// The bounds of the positions, without skinning or morph targets.
    pub fn bounds(&self) -> Aabb {
        self.bounds
    }

    /// Calculates [`Geometry::bounds`] again. Call it after you change `positions`.
    pub fn update_bounds(&mut self) {
        self.bounds = Aabb::from_points(self.positions.iter().copied());
    }

    /// Calculates smooth normals from the triangles, weighted by triangle area, like
    /// three.js `computeVertexNormals`.
    pub fn compute_vertex_normals(&mut self) {
        let mut normals = vec![Vec3::ZERO; self.positions.len()];
        for tri in self.triangles() {
            let corners = tri.map(|i| i as usize);
            let n = face_normal(corners.map(|i| self.positions[i]));
            for i in corners {
                normals[i] += n;
            }
        }
        for n in &mut normals {
            *n = n.normalize_or(Vec3::Y);
        }
        self.normals = normals;
    }

    /// A box, like three.js `BoxGeometry(width, height, depth)`.
    pub fn cuboid(width: f32, height: f32, depth: f32) -> Self {
        Self::rounded_box(width, height, depth, 0.0, 1)
    }

    /// A box with rounded edges and corners, like three.js `RoundedBoxGeometry`. `radius` is
    /// the radius of the edges. `segments` is the number of steps on each rounded edge; 3
    /// or 4 look smooth. Rounded edges catch highlights, so use them for machined parts.
    /// A negative size gives the same box as its absolute value.
    pub fn rounded_box(width: f32, height: f32, depth: f32, radius: f32, segments: u32) -> Self {
        let half = Vec3::new(width, height, depth).abs() * 0.5;
        let radius = radius.min(half.min_element()).max(0.0);
        let inner = half - Vec3::splat(radius);
        let segments = segments.max(1);
        // Grid lines along a face axis with the half size `h`. A line at `radius * tan(a)`
        // past the flat part moves to the angle `a` on the rounded edge. Each of the two
        // faces at an edge thus covers 45 degrees of it in equal steps.
        let axis_coords = |h: f32| -> Vec<f32> {
            if radius <= 0.0 {
                return vec![-h, h];
            }
            let inner = h - radius;
            let step = |k: u32| radius * (k as f32 / segments as f32 * FRAC_PI_4).tan();
            let mut coords: Vec<f32> = (1..=segments).rev().map(|k| -inner - step(k)).collect();
            coords.push(-inner);
            if inner > 1e-6 {
                coords.push(inner);
            }
            coords.extend((1..=segments).map(|k| inner + step(k)));
            coords
        };

        // The normal of each face, then its U and V directions.
        let faces = [
            (Vec3::X, Vec3::NEG_Z, Vec3::Y),
            (Vec3::NEG_X, Vec3::Z, Vec3::Y),
            (Vec3::Y, Vec3::X, Vec3::NEG_Z),
            (Vec3::NEG_Y, Vec3::X, Vec3::Z),
            (Vec3::Z, Vec3::X, Vec3::Y),
            (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
        ];

        let mut b = Builder::default();
        for (n, u, v) in faces {
            let [hn, hu, hv] = [n, u, v].map(|axis| axis.abs().dot(half));
            let (us, vs) = (axis_coords(hu), axis_coords(hv));
            let base = b.positions.len() as u32;
            for &cv in &vs {
                for &cu in &us {
                    let p = n * hn + u * cu + v * cv;
                    let (p, normal) = if radius > 0.0 {
                        // Move the point from the inner box out to the rounded surface.
                        let c = p.clamp(-inner, inner);
                        let normal = (p - c).normalize_or(n);
                        (c + normal * radius, normal)
                    } else {
                        (p, n)
                    };
                    // a side of zero length gets UV 0 instead of 0 / 0
                    let (su, sv) = (
                        (2.0 * hu).max(f32::MIN_POSITIVE),
                        (2.0 * hv).max(f32::MIN_POSITIVE),
                    );
                    let uv = Vec2::new((cu + hu) / su, 1.0 - (cv + hv) / sv);
                    b.vertex(p, normal, uv);
                }
            }
            b.grid(base, us.len() as u32, vs.len() as u32);
        }
        b.build()
    }

    /// A UV sphere, like three.js `SphereGeometry(radius, widthSegments, heightSegments)`.
    pub fn sphere(radius: f32, width_segments: u32, height_segments: u32) -> Self {
        Self::sphere_partial(radius, width_segments, height_segments, 0.0, PI)
    }

    /// The part of a sphere between the polar angles `theta_start` and
    /// `theta_start + theta_length`, in radians. The angle 0 is the top.
    /// `sphere_partial(r, 32, 16, 0.0, PI / 2.0)` is a dome.
    pub fn sphere_partial(
        radius: f32,
        width_segments: u32,
        height_segments: u32,
        theta_start: f32,
        theta_length: f32,
    ) -> Self {
        let ws = width_segments.max(3);
        let hs = height_segments.max(2);
        let theta_end = (theta_start + theta_length).min(PI);
        let mut b = Builder::default();
        for iy in 0..=hs {
            let v = iy as f32 / hs as f32;
            let theta = theta_start + v * theta_length;
            for ix in 0..=ws {
                let u = ix as f32 / ws as f32;
                let phi = u * TAU;
                let n = Vec3::new(
                    -phi.cos() * theta.sin(),
                    theta.cos(),
                    phi.sin() * theta.sin(),
                );
                // V runs down the image, so the top of a panorama is at the north pole
                b.vertex(n * radius, n, Vec2::new(u, v));
            }
        }
        let row = ws + 1;
        for iy in 0..hs {
            for ix in 0..ws {
                let a = iy * row + ix;
                // At a pole one triangle of each quad has no area, so skip it.
                if iy != 0 || theta_start > 0.0 {
                    b.tri(a + 1, a, a + row + 1);
                }
                if iy != hs - 1 || theta_end < PI {
                    b.tri(a, a + row, a + row + 1);
                }
            }
        }
        b.build()
    }

    /// A closed cylinder along Y, like three.js `CylinderGeometry(radiusTop, radiusBottom,
    /// height, radialSegments)`. A radius of 0 makes a cone. The side has V from 0 at the
    /// top to 1 at the bottom, and the caps have planar UVs.
    pub fn cylinder(
        radius_top: f32,
        radius_bottom: f32,
        height: f32,
        radial_segments: u32,
    ) -> Self {
        let h = height * 0.5;
        let side = Self::cylinder_open(radius_top, radius_bottom, height, radial_segments);
        let caps = [(radius_top, h, true), (radius_bottom, -h, false)]
            .into_iter()
            .filter(|(radius, _, _)| *radius > 0.0)
            .map(|(radius, y, top)| cap(radius, y, top, radial_segments));
        Self::merge(std::iter::once(side).chain(caps))
    }

    /// A cylinder along Y without caps. The parameters are the same as for
    /// [`Geometry::cylinder`].
    pub fn cylinder_open(
        radius_top: f32,
        radius_bottom: f32,
        height: f32,
        radial_segments: u32,
    ) -> Self {
        let h = height * 0.5;
        Self::lathe(
            &[Vec2::new(radius_bottom, -h), Vec2::new(radius_top, h)],
            radial_segments,
        )
    }

    /// A cone along Y with its tip up, like three.js `ConeGeometry`.
    pub fn cone(radius: f32, height: f32, radial_segments: u32) -> Self {
        Self::cylinder(0.0, radius, height, radial_segments)
    }

    /// A capsule along Y, like three.js `CapsuleGeometry(radius, length)`. `length` is the
    /// straight part between the two half spheres.
    pub fn capsule(radius: f32, length: f32, cap_segments: u32, radial_segments: u32) -> Self {
        let cap_segments = cap_segments.max(2);
        let h = length * 0.5;
        let angle = |i: u32| i as f32 / cap_segments as f32 * FRAC_PI_2;
        let point = |a: f32, y: f32| Vec2::new(radius * a.cos(), y + radius * a.sin());
        let bottom = (0..=cap_segments).map(|i| point(-FRAC_PI_2 + angle(i), -h));
        let top = (0..=cap_segments).map(|i| point(angle(i), h));
        Self::lathe(&bottom.chain(top).collect::<Vec<_>>(), radial_segments)
    }

    /// A torus around the Z axis, like three.js `TorusGeometry(radius, tube)`.
    pub fn torus(radius: f32, tube: f32, radial_segments: u32, tubular_segments: u32) -> Self {
        Self::torus_arc(radius, tube, radial_segments, tubular_segments, TAU)
    }

    /// The part of a torus from angle 0 to `arc` radians.
    pub fn torus_arc(
        radius: f32,
        tube: f32,
        radial_segments: u32,
        tubular_segments: u32,
        arc: f32,
    ) -> Self {
        let rs = radial_segments.max(3);
        let ts = tubular_segments.max(3);
        let mut b = Builder::default();
        for j in 0..=rs {
            let v = j as f32 / rs as f32 * TAU;
            for i in 0..=ts {
                let u = i as f32 / ts as f32 * arc;
                let center = Vec3::new(radius * u.cos(), radius * u.sin(), 0.0);
                let p = Vec3::new(
                    (radius + tube * v.cos()) * u.cos(),
                    (radius + tube * v.cos()) * u.sin(),
                    tube * v.sin(),
                );
                let uv = Vec2::new(i as f32 / ts as f32, 1.0 - j as f32 / rs as f32);
                b.vertex(p, (p - center).normalize_or_zero(), uv);
            }
        }
        let row = ts + 1;
        for j in 0..rs {
            for i in 0..ts {
                let a = j * row + i;
                b.tri(a + row, a, a + row + 1);
                b.tri(a, a + 1, a + row + 1);
            }
        }
        b.build()
    }

    /// A rectangle in the XY plane that faces +Z, like three.js `PlaneGeometry`.
    pub fn plane(width: f32, height: f32) -> Self {
        let (w, h) = (width * 0.5, height * 0.5);
        let mut b = Builder::default();
        b.vertex(Vec3::new(-w, -h, 0.0), Vec3::Z, Vec2::new(0.0, 1.0));
        b.vertex(Vec3::new(w, -h, 0.0), Vec3::Z, Vec2::new(1.0, 1.0));
        b.vertex(Vec3::new(w, h, 0.0), Vec3::Z, Vec2::new(1.0, 0.0));
        b.vertex(Vec3::new(-w, h, 0.0), Vec3::Z, Vec2::new(0.0, 0.0));
        b.quad(0, 1, 2, 3);
        b.build()
    }

    /// A floor: a rectangle in the XZ plane that faces up (+Y). The UVs are in world
    /// units, so textures and grid patterns repeat once per unit.
    pub fn floor(width: f32, depth: f32) -> Self {
        Self::floor_segments(width, depth, 1, 1)
    }

    /// A [`Geometry::floor`] made of a grid of quads, for vertex colors or deformation.
    pub fn floor_segments(
        width: f32,
        depth: f32,
        width_segments: u32,
        depth_segments: u32,
    ) -> Self {
        let (ws, ds) = (width_segments.max(1), depth_segments.max(1));
        let mut b = Builder::default();
        for j in 0..=ds {
            let z = depth * 0.5 - depth * j as f32 / ds as f32;
            for i in 0..=ws {
                let x = -width * 0.5 + width * i as f32 / ws as f32;
                b.vertex(Vec3::new(x, 0.0, z), Vec3::Y, Vec2::new(x, z));
            }
        }
        b.grid(0, ws + 1, ds + 1);
        b.build()
    }

    /// A disc in the XY plane that faces +Z, like three.js `CircleGeometry`.
    pub fn circle(radius: f32, segments: u32) -> Self {
        Self::ring(0.0, radius, segments)
    }

    /// A flat ring in the XY plane that faces +Z, like three.js `RingGeometry`.
    pub fn ring(inner: f32, outer: f32, segments: u32) -> Self {
        let segments = segments.max(3);
        let mut b = Builder::default();
        for i in 0..=segments {
            let a = i as f32 / segments as f32 * TAU;
            let dir = Vec3::new(a.cos(), a.sin(), 0.0);
            let ratio = inner / outer.max(f32::MIN_POSITIVE);
            let inner_uv = Vec2::new(0.5 + 0.5 * dir.x * ratio, 0.5 - 0.5 * dir.y * ratio);
            b.vertex(dir * inner, Vec3::Z, inner_uv);
            let outer_uv = Vec2::new(0.5 + 0.5 * dir.x, 0.5 - 0.5 * dir.y);
            b.vertex(dir * outer, Vec3::Z, outer_uv);
        }
        for i in 0..segments {
            let a = i * 2;
            b.quad(a, a + 1, a + 3, a + 2);
        }
        b.build()
    }

    /// Turns a profile around the Y axis, like three.js `LatheGeometry`. The points are
    /// `(radius, y)` from bottom to top. Corners sharper than 35 degrees get a separate
    /// normal on each side. Thus the profile `[(0, 0), (1, 0), (1, 1), (0, 1)]` is a
    /// cylinder with sharp edges. V goes along the profile, from 1 at the first point to 0
    /// at the last, in proportion to the length. Fewer than two points give an empty
    /// geometry.
    pub fn lathe(points: &[Vec2], segments: u32) -> Self {
        let segments = segments.max(3);
        let profile = split_sharp_profile(points, 35f32.to_radians());
        if profile.len() < 2 {
            return Geometry::default();
        }
        let n = profile.len() as u32;
        let mut along = vec![0.0f32; profile.len()];
        for j in 1..profile.len() {
            along[j] = along[j - 1] + profile[j].0.distance(profile[j - 1].0);
        }
        let length = along[profile.len() - 1].max(f32::MIN_POSITIVE);
        let mut b = Builder::default();
        for i in 0..=segments {
            let phi = i as f32 / segments as f32 * TAU;
            let (sin, cos) = phi.sin_cos();
            for (j, (p, normal)) in profile.iter().enumerate() {
                let position = Vec3::new(p.x * sin, p.y, p.x * cos);
                let normal =
                    Vec3::new(normal.x * sin, normal.y, normal.x * cos).normalize_or(Vec3::Y);
                let uv = Vec2::new(i as f32 / segments as f32, 1.0 - along[j] / length);
                b.vertex(position, normal, uv);
            }
        }
        for i in 0..segments {
            for j in 0..n - 1 {
                // A split corner has two points at the same place. The quad between them
                // has no area, so skip it.
                if profile[j as usize].0 == profile[j as usize + 1].0 {
                    continue;
                }
                let a = j + i * n;
                b.tri(a, a + n, a + 1);
                b.tri(a + n + 1, a + 1, a + n);
            }
        }
        b.build()
    }

    /// Extrudes a 2D shape along Z, like three.js `ExtrudeGeometry`. The shape is a list of
    /// counter-clockwise points in the XY plane, and it can be concave. `depth` is the
    /// thickness, centered on Z = 0. `bevel` is the radius of the rounded front and back
    /// edges, which make plates and panels look machined.
    pub fn extrude(shape: &[Vec2], depth: f32, bevel: f32) -> Self {
        Self::extrude_segments(shape, depth, bevel, 3)
    }

    /// A [`Geometry::extrude`] with `bevel_segments` steps on each bevel. 1 step makes a
    /// chamfer.
    pub fn extrude_segments(shape: &[Vec2], depth: f32, bevel: f32, bevel_segments: u32) -> Self {
        let shape = Shape {
            outer: shape.to_vec(),
            holes: Vec::new(),
        };
        Self::extrude_shape(&shape, depth, bevel, bevel_segments)
    }

    /// Extrudes a [`Shape`] with holes along Z, centered on Z = 0. The bevel has
    /// `bevel_segments` rounded steps. [`Geometry::text`] makes letters with it.
    pub fn extrude_shape(shape: &Shape, depth: f32, bevel: f32, bevel_segments: u32) -> Self {
        // The outer contour goes counter-clockwise and the holes go clockwise. The solid is
        // thus always on the left side of an edge.
        let mut contours = Vec::with_capacity(1 + shape.holes.len());
        for (i, contour) in std::iter::once(&shape.outer)
            .chain(&shape.holes)
            .enumerate()
        {
            let mut c = contour.clone();
            c.dedup_by(|a, b| a.distance_squared(*b) < 1e-12);
            if let [first, .., last] = c.as_slice()
                && first.distance_squared(*last) < 1e-12
            {
                c.pop();
            }
            if c.len() < 3 {
                if i == 0 {
                    return Geometry::default();
                }
                continue;
            }
            if (signed_area(&c) > 0.0) != (i == 0) {
                c.reverse();
            }
            contours.push(c);
        }
        let half = depth.abs() * 0.5;
        let bevel = bevel.min(half).max(0.0);
        let bevel_segments = if bevel > 0.0 {
            bevel_segments.max(1)
        } else {
            0
        };

        // The side wall has rings of points from the front (+Z) to the back. On the bevel,
        // ring `k` is at the angle `a` from the cap plane.
        let ring = |k: u32, back: bool| {
            let a = if bevel_segments == 0 {
                FRAC_PI_2
            } else {
                k as f32 / bevel_segments as f32 * FRAC_PI_2
            };
            let (sin, cos) = a.sin_cos();
            WallRing {
                inset: bevel * (1.0 - sin),
                z: if back {
                    -half + bevel - bevel * cos
                } else {
                    half - bevel + bevel * cos
                },
                normal_z: if back { -cos } else { cos },
                normal_xy: sin,
            }
        };
        let rings: Vec<WallRing> = (0..=bevel_segments)
            .map(|k| ring(k, false))
            .chain((0..=bevel_segments).rev().map(|k| ring(k, true)))
            .collect();

        // Moves corner `i` of contour `c` into the solid by `amount`, along the miter of
        // its two edges.
        let inset = |c: &[Vec2], i: usize, amount: f32| -> Vec2 {
            if amount == 0.0 {
                return c[i];
            }
            let n = c.len();
            let n0 = (c[i] - c[(i + n - 1) % n]).perp().normalize_or_zero();
            let n1 = (c[(i + 1) % n] - c[i]).perp().normalize_or_zero();
            let miter = (n0 + n1).normalize_or(n1);
            // The limit keeps sharp corners from going too far.
            c[i] + miter * (amount / miter.dot(n1).max(0.25))
        };

        let mut b = Builder::default();
        // Side walls: flat along each edge, smooth across the bevel.
        for c in &contours {
            let perimeter = closed_edges(c)
                .map(|(p, q)| p.distance(q))
                .sum::<f32>()
                .max(1e-6);
            let mut along = 0.0;
            for i in 0..c.len() {
                let j = (i + 1) % c.len();
                let outward = -(c[j] - c[i]).perp().normalize_or_zero();
                let edge_len = c[i].distance(c[j]);
                let base = b.positions.len() as u32;
                for (r, ring) in rings.iter().enumerate() {
                    let normal = (outward * ring.normal_xy)
                        .extend(ring.normal_z)
                        .normalize_or(Vec3::Z);
                    let v = r as f32 / (rings.len() - 1) as f32;
                    let p0 = inset(c, i, ring.inset);
                    let p1 = inset(c, j, ring.inset);
                    b.vertex(p0.extend(ring.z), normal, Vec2::new(along / perimeter, v));
                    b.vertex(
                        p1.extend(ring.z),
                        normal,
                        Vec2::new((along + edge_len) / perimeter, v),
                    );
                }
                along += edge_len;
                for r in 0..rings.len() as u32 - 1 {
                    // Ring `r` is in front of ring `r + 1`, which sets the winding.
                    let a = base + r * 2;
                    b.quad(a + 2, a + 3, a + 1, a);
                }
            }
        }

        // Caps: the inset contours, filled with the non-zero winding rule because font
        // contours can overlap.
        let insets: Vec<Vec<Vec2>> = contours
            .iter()
            .map(|c| (0..c.len()).map(|i| inset(c, i, bevel)).collect())
            .collect();
        let triangles = fill_nonzero(&insets);
        let bounds = Aabb::from_points(insets.iter().flatten().map(|p| p.extend(0.0)));
        let min = bounds.min.truncate();
        let size = bounds.size().truncate().max(Vec2::splat(1e-6));
        for (z, normal) in [(half, Vec3::Z), (-half, Vec3::NEG_Z)] {
            for tri in &triangles {
                let base = b.positions.len() as u32;
                for p in tri {
                    let uv = (*p - min) / size;
                    b.vertex(p.extend(z), normal, Vec2::new(uv.x, 1.0 - uv.y));
                }
                // The back cap faces -Z, so it has the opposite winding.
                if normal.z > 0.0 {
                    b.tri(base, base + 1, base + 2);
                } else {
                    b.tri(base, base + 2, base + 1);
                }
            }
        }
        b.build()
    }

    /// The outlines of `text` in a TrueType or OpenType font, as [`Shape`]s for each glyph.
    /// `size` is the font size (one em) in world units, and `letter_spacing` is extra space
    /// after each glyph. Each glyph has an origin: the pen position on the baseline. This
    /// function does not apply kerning.
    pub fn text_shapes(
        font: &[u8],
        text: &str,
        size: f32,
        letter_spacing: f32,
    ) -> Result<Vec<GlyphShapes>, TextError> {
        let face = fframes::ttf_parser::Face::parse(font, 0).map_err(|_| TextError::InvalidFont)?;
        let scale = size / f32::from(face.units_per_em());
        let mut pen = 0.0;
        text.chars()
            .map(|ch| {
                let id = face.glyph_index(ch).ok_or(TextError::MissingGlyph(ch))?;
                let mut outline = OutlineCollector {
                    scale,
                    ..Default::default()
                };
                face.outline_glyph(id, &mut outline);
                outline.finish();
                let advance = f32::from(face.glyph_hor_advance(id).unwrap_or(0)) * scale;
                let glyph = GlyphShapes {
                    ch,
                    origin: Vec2::new(pen, 0.0),
                    advance,
                    shapes: shapes_from_contours(outline.contours),
                };
                pen += advance + letter_spacing;
                Ok(glyph)
            })
            .collect()
    }

    /// Extruded 3D text, like three.js `TextGeometry`. `size` is the em size, `depth` the
    /// thickness, and `bevel` the radius of the rounded edges. The text starts at X = 0 on
    /// the baseline (Y = 0) and faces +Z. For one geometry per letter, use
    /// [`Geometry::text_glyphs`].
    pub fn text(
        font: &[u8],
        text: &str,
        size: f32,
        depth: f32,
        bevel: f32,
    ) -> Result<Self, TextError> {
        Ok(Self::merge(
            Self::text_glyphs(font, text, size, depth, bevel)?
                .into_iter()
                .map(|g| g.geometry.translate(g.origin.x, g.origin.y, 0.0)),
        ))
    }

    /// Each letter of `text` as a separate extruded geometry, with its position in the line.
    /// The parameters are the same as for [`Geometry::text`].
    pub fn text_glyphs(
        font: &[u8],
        text: &str,
        size: f32,
        depth: f32,
        bevel: f32,
    ) -> Result<Vec<Glyph>, TextError> {
        Ok(Self::text_shapes(font, text, size, 0.0)?
            .into_iter()
            .map(|g| Glyph {
                ch: g.ch,
                origin: g.origin,
                advance: g.advance,
                geometry: Self::merge(
                    g.shapes
                        .iter()
                        .map(|s| Self::extrude_shape(s, depth, bevel, 2)),
                ),
            })
            .collect())
    }

    /// A regular polygon in the XY plane for [`Geometry::extrude`], with `sides` corners at
    /// the distance `radius` from the center. The first corner points up.
    pub fn polygon(radius: f32, sides: u32) -> Vec<Vec2> {
        let sides = sides.max(3);
        (0..sides)
            .map(|i| {
                let a = FRAC_PI_2 + i as f32 / sides as f32 * TAU;
                Vec2::new(a.cos(), a.sin()) * radius
            })
            .collect()
    }

    /// An icosphere, like three.js `IcosahedronGeometry(radius, detail)`. Each face splits
    /// into `(detail + 1)²` triangles. Detail 0 is faceted, higher details have smooth
    /// normals; [`crate::Material::flat`] makes them faceted too.
    pub fn icosahedron(radius: f32, detail: u32) -> Self {
        let t = f32::midpoint(1.0, 5f32.sqrt());
        let corners: Vec<Vec3> = [
            (-1.0, t, 0.0),
            (1.0, t, 0.0),
            (-1.0, -t, 0.0),
            (1.0, -t, 0.0),
            (0.0, -1.0, t),
            (0.0, 1.0, t),
            (0.0, -1.0, -t),
            (0.0, 1.0, -t),
            (t, 0.0, -1.0),
            (t, 0.0, 1.0),
            (-t, 0.0, -1.0),
            (-t, 0.0, 1.0),
        ]
        .iter()
        .map(|&(x, y, z)| Vec3::new(x, y, z).normalize())
        .collect();
        const FACES: [[usize; 3]; 20] = [
            [0, 11, 5],
            [0, 5, 1],
            [0, 1, 7],
            [0, 7, 10],
            [0, 10, 11],
            [1, 5, 9],
            [5, 11, 4],
            [11, 10, 2],
            [10, 7, 6],
            [7, 1, 8],
            [3, 9, 4],
            [3, 4, 2],
            [3, 2, 6],
            [3, 6, 8],
            [3, 8, 9],
            [4, 9, 5],
            [2, 4, 11],
            [6, 2, 10],
            [8, 6, 7],
            [9, 8, 1],
        ];
        // three.js `PolyhedronGeometry`: a triangular grid on each face, with its own
        // vertices, so the UV seam of each triangle can be fixed
        let cols = detail as usize + 1;
        let mut triangles: Vec<[Vec3; 3]> = Vec::with_capacity(20 * cols * cols);
        for [a, b, c] in FACES {
            let (a, b, c) = (corners[a], corners[b], corners[c]);
            let grid: Vec<Vec<Vec3>> = (0..=cols)
                .map(|i| {
                    let aj = a.lerp(c, i as f32 / cols as f32);
                    let bj = b.lerp(c, i as f32 / cols as f32);
                    let rows = cols - i;
                    (0..=rows)
                        .map(|j| {
                            if rows == 0 {
                                aj
                            } else {
                                aj.lerp(bj, j as f32 / rows as f32)
                            }
                        })
                        .collect()
                })
                .collect();
            for i in 0..cols {
                for j in 0..2 * (cols - i) - 1 {
                    let k = j / 2;
                    triangles.push(if j % 2 == 0 {
                        [grid[i][k + 1], grid[i + 1][k], grid[i][k]]
                    } else {
                        [grid[i][k + 1], grid[i + 1][k + 1], grid[i + 1][k]]
                    });
                }
            }
        }
        let azimuth = |v: Vec3| v.z.atan2(-v.x);
        let mut b = Builder::default();
        for tri in triangles {
            let tri = tri.map(Vec3::normalize);
            let mut uvs = tri.map(|v| Vec2::new(azimuth(v) / TAU + 0.5, 0.5 - v.y.asin() / PI));
            // a vertex on a pole takes the azimuth of the triangle center
            let center = (tri[0] + tri[1] + tri[2]) / 3.0;
            for (uv, v) in uvs.iter_mut().zip(tri) {
                if v.x.abs() < 1e-6 && v.z.abs() < 1e-6 {
                    uv.x = azimuth(center) / TAU + 0.5;
                }
            }
            // a triangle across the seam at U 0 and 1 wraps its small Us
            let (min, max) = uvs.iter().fold((f32::MAX, f32::MIN), |(lo, hi), uv| {
                (lo.min(uv.x), hi.max(uv.x))
            });
            if max > 0.9 && min < 0.1 {
                for uv in &mut uvs {
                    if uv.x < 0.2 {
                        uv.x += 1.0;
                    }
                }
            }
            // three.js: flat normals without subdivision, sphere normals with it
            let flat = face_normal(tri).normalize_or(center);
            let i = b.positions.len() as u32;
            for (v, uv) in tri.iter().zip(uvs) {
                b.vertex(*v * radius, if detail == 0 { flat } else { *v }, uv);
            }
            b.tri(i, i + 1, i + 2);
        }
        b.build()
    }

    /// A round tube with `radius` along `path`, like three.js `TubeGeometry`. Use it for
    /// cables and hoses. The tube goes straight from point to point, so give enough points
    /// for a smooth curve. A path with less than two points gives an empty geometry.
    pub fn tube(path: &[Vec3], radius: f32, radial_segments: u32) -> Self {
        let rs = radial_segments.max(3);
        if path.len() < 2 {
            return Geometry::default();
        }
        let tangents: Vec<Vec3> = (0..path.len())
            .map(|i| {
                let a = path[i.saturating_sub(1)];
                let b = path[(i + 1).min(path.len() - 1)];
                (b - a).normalize_or(Vec3::Y)
            })
            .collect();
        // Parallel transport: the normal turns with the tangent from one point to the next,
        // so the tube does not twist.
        let mut normal = tangents[0].any_orthonormal_vector();
        let mut b = Builder::default();
        for (i, p) in path.iter().enumerate() {
            if i > 0 {
                let axis = tangents[i - 1].cross(tangents[i]);
                if axis.length_squared() > 1e-10 {
                    let angle = tangents[i - 1].dot(tangents[i]).clamp(-1.0, 1.0).acos();
                    normal = Quat::from_axis_angle(axis.normalize(), angle) * normal;
                }
            }
            let binormal = tangents[i].cross(normal);
            for j in 0..=rs {
                let a = j as f32 / rs as f32 * TAU;
                let n = (normal * a.cos() + binormal * a.sin()).normalize();
                let uv = Vec2::new(
                    i as f32 / (path.len() - 1) as f32,
                    1.0 - j as f32 / rs as f32,
                );
                b.vertex(*p + n * radius, n, uv);
            }
        }
        b.grid(0, rs + 1, path.len() as u32);
        b.build()
    }

    /// Applies the matrix `m` to the positions, the normals and the morph targets. When `m`
    /// mirrors the geometry, this function also reverses the winding, so the triangles
    /// still face out.
    pub fn transform(mut self, m: Mat4) -> Self {
        let linear = Mat3::from_mat4(m);
        let normal_matrix = linear.inverse().transpose();
        for p in &mut self.positions {
            *p = m.transform_point3(*p);
        }
        for n in &mut self.normals {
            *n = (normal_matrix * *n).normalize_or_zero();
        }
        // the targets hold offsets: no translation, and normal offsets stay unnormalized
        for target in &mut self.morph_targets {
            for d in &mut target.positions {
                *d = linear * *d;
            }
            for d in &mut target.normals {
                *d = normal_matrix * *d;
            }
        }
        if m.determinant() < 0.0 {
            for tri in self.indices.as_chunks_mut::<3>().0 {
                tri.swap(1, 2);
            }
        }
        self.update_bounds();
        self
    }

    /// Moves the geometry by `(x, y, z)`.
    pub fn translate(self, x: f32, y: f32, z: f32) -> Self {
        self.transform(Mat4::from_translation(Vec3::new(x, y, z)))
    }

    /// Rotates the geometry around the origin.
    pub fn rotate(self, rotation: Quat) -> Self {
        self.transform(Mat4::from_quat(rotation))
    }

    /// Rotates the geometry around the X axis by `angle` radians.
    pub fn rotate_x(self, angle: f32) -> Self {
        self.transform(Mat4::from_rotation_x(angle))
    }

    /// Rotates the geometry around the Y axis by `angle` radians.
    pub fn rotate_y(self, angle: f32) -> Self {
        self.transform(Mat4::from_rotation_y(angle))
    }

    /// Rotates the geometry around the Z axis by `angle` radians.
    pub fn rotate_z(self, angle: f32) -> Self {
        self.transform(Mat4::from_rotation_z(angle))
    }

    /// Scales the geometry along each axis. A negative factor mirrors it.
    pub fn scale(self, x: f32, y: f32, z: f32) -> Self {
        self.transform(Mat4::from_scale(Vec3::new(x, y, z)))
    }

    /// Sets all vertex colors to `color`. Then you can merge parts of different colors
    /// into one mesh with one material. The material color multiplies the vertex color.
    pub fn with_color(mut self, color: impl Into<Rgb>) -> Self {
        let c = color.into().0.extend(1.0);
        self.colors = vec![c; self.positions.len()];
        self
    }

    /// Joins geometries into one. When only some parts have UVs or vertex colors, the
    /// other parts get zero UVs and white vertex colors. The result has no skin and no
    /// morph targets.
    pub fn merge(parts: impl IntoIterator<Item = Geometry>) -> Self {
        let parts: Vec<Geometry> = parts.into_iter().collect();
        let any_uvs = parts.iter().any(|p| !p.uvs.is_empty());
        let any_colors = parts.iter().any(|p| !p.colors.is_empty());
        let mut out = Geometry::default();
        for part in parts {
            let offset = out.positions.len() as u32;
            let count = part.positions.len();
            out.positions.extend(part.positions);
            out.normals.extend(part.normals);
            if any_uvs {
                if part.uvs.is_empty() {
                    out.uvs.extend(std::iter::repeat_n(Vec2::ZERO, count));
                } else {
                    out.uvs.extend(part.uvs);
                }
            }
            if any_colors {
                if part.colors.is_empty() {
                    out.colors.extend(std::iter::repeat_n(Vec4::ONE, count));
                } else {
                    out.colors.extend(part.colors);
                }
            }
            out.indices.extend(part.indices.iter().map(|i| i + offset));
        }
        out.update_bounds();
        out
    }

    /// Gives each triangle its own vertices with the face normal. The result looks faceted
    /// with any material. It has no skin and no morph targets.
    pub fn flat(self) -> Self {
        let mut b = Builder::default();
        for tri in self.triangles() {
            let n = face_normal(tri.map(|i| self.positions[i as usize])).normalize_or_zero();
            let [i0, i1, i2] = tri.map(|i| b.copy_vertex(&self, i as usize, n));
            b.tri(i0, i1, i2);
        }
        b.build()
    }
}

/// Collects vertices and triangles for a new [`Geometry`].
#[derive(Default)]
struct Builder {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    uvs: Vec<Vec2>,
    colors: Vec<Vec4>,
    indices: Vec<u32>,
}

impl Builder {
    /// Adds a vertex and returns its index.
    fn vertex(&mut self, p: Vec3, n: Vec3, uv: Vec2) -> u32 {
        self.positions.push(p);
        self.normals.push(n);
        self.uvs.push(uv);
        self.positions.len() as u32 - 1
    }

    /// Adds a copy of vertex `i` of `source` with the normal `n`, and returns its index.
    /// The copy has a UV and a color only when `source` has them.
    fn copy_vertex(&mut self, source: &Geometry, i: usize, n: Vec3) -> u32 {
        self.positions.push(source.positions[i]);
        self.normals.push(n);
        if !source.uvs.is_empty() {
            self.uvs
                .push(source.uvs.get(i).copied().unwrap_or_default());
        }
        if let Some(&color) = source.colors.get(i) {
            self.colors.push(color);
        }
        self.positions.len() as u32 - 1
    }

    /// Adds a triangle with three new vertices. All of them have the normal `n` and a zero
    /// UV.
    fn flat_triangle(&mut self, [a, b, c]: [Vec3; 3], n: Vec3) {
        let i = self.vertex(a, n, Vec2::ZERO);
        self.vertex(b, n, Vec2::ZERO);
        self.vertex(c, n, Vec2::ZERO);
        self.tri(i, i + 1, i + 2);
    }

    fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend([a, b, c]);
    }

    /// Adds the counter-clockwise quad `a b c d` as two triangles.
    fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.indices.extend([a, b, c, a, c, d]);
    }

    /// Adds two triangles for each cell of a vertex grid. The grid starts at index `base`
    /// and has `rows` rows of `columns` vertices.
    fn grid(&mut self, base: u32, columns: u32, rows: u32) {
        for j in 0..rows - 1 {
            for i in 0..columns - 1 {
                let a = base + j * columns + i;
                self.quad(a, a + 1, a + columns + 1, a + columns);
            }
        }
    }

    fn build(self) -> Geometry {
        let mut geometry = Geometry::new(self.positions, self.normals, self.uvs, self.indices);
        geometry.colors = self.colors;
        geometry
    }
}

/// A cap of [`Geometry::cylinder`]: a disc of `radius` at the height `y` that faces up
/// (`top`) or down. Its rim has the vertices of the side, so the two meet without a gap.
fn cap(radius: f32, y: f32, top: bool, segments: u32) -> Geometry {
    let segments = segments.max(3);
    let normal = if top { Vec3::Y } else { Vec3::NEG_Y };
    let mut b = Builder::default();
    let center = b.vertex(Vec3::new(0.0, y, 0.0), normal, Vec2::splat(0.5));
    for i in 0..=segments {
        let (sin, cos) = (i as f32 / segments as f32 * TAU).sin_cos();
        // seen from outside, U goes to +X and V to the far side of the bottom cap
        let v = if top {
            0.5 + 0.5 * cos
        } else {
            0.5 - 0.5 * cos
        };
        b.vertex(
            Vec3::new(radius * sin, y, radius * cos),
            normal,
            Vec2::new(0.5 + 0.5 * sin, v),
        );
    }
    for i in 1..=segments {
        if top {
            b.tri(center, i, i + 1);
        } else {
            b.tri(center, i + 1, i);
        }
    }
    b.build()
}

/// A ring of points on the side wall of an extrusion, see [`Geometry::extrude_shape`].
struct WallRing {
    /// The distance that the contour moves into the solid.
    inset: f32,
    z: f32,
    /// The Z part of the normal before normalization.
    normal_z: f32,
    /// The weight of the outward edge normal in the normal.
    normal_xy: f32,
}

/// The cross product of two triangle edges. It points to the counter-clockwise side, and
/// its length is two times the triangle area.
fn face_normal([a, b, c]: [Vec3; 3]) -> Vec3 {
    (b - a).cross(c - a)
}

/// The edges of a closed contour: each point with the next point, and the last point with
/// the first.
fn closed_edges(contour: &[Vec2]) -> impl Iterator<Item = (Vec2, Vec2)> {
    let points = contour.iter().copied();
    points.clone().zip(points.cycle().skip(1))
}

/// The area of a polygon. It is positive when the points go counter-clockwise.
fn signed_area(points: &[Vec2]) -> f32 {
    closed_edges(points)
        .map(|(a, b)| a.perp_dot(b))
        .sum::<f32>()
        * 0.5
}

/// Pairs each point of a lathe profile with its 2D normal. A corner sharper than
/// `max_angle` occurs two times, once with the normal of each segment.
fn split_sharp_profile(points: &[Vec2], max_angle: f32) -> Vec<(Vec2, Vec2)> {
    let n = points.len();
    if n < 2 {
        return Vec::new();
    }
    // The normal on the right side of segment `i`. It points away from the axis when the
    // profile goes up.
    let seg_normal = |i: usize| -> Vec2 {
        let d = points[i + 1] - points[i];
        Vec2::new(d.y, -d.x).normalize_or_zero()
    };
    let mut out = Vec::with_capacity(n + 4);
    out.push((points[0], seg_normal(0)));
    for i in 1..n - 1 {
        let a = seg_normal(i - 1);
        let b = seg_normal(i);
        if a == Vec2::ZERO || b == Vec2::ZERO || a.angle_to(b).abs() > max_angle {
            out.push((points[i], if a == Vec2::ZERO { b } else { a }));
            out.push((points[i], if b == Vec2::ZERO { a } else { b }));
        } else {
            out.push((points[i], (a + b).normalize_or(a)));
        }
    }
    out.push((points[n - 1], seg_normal(n - 2)));
    out
}

/// A filled outline with holes, for [`Geometry::extrude_shape`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shape {
    pub outer: Vec<Vec2>,
    pub holes: Vec<Vec<Vec2>>,
}

/// The outlines of one letter from [`Geometry::text_shapes`].
#[derive(Debug, Clone)]
pub struct GlyphShapes {
    pub ch: char,
    /// The pen position of the glyph on the baseline.
    pub origin: Vec2,
    /// The horizontal distance from this glyph to the next, without letter spacing.
    pub advance: f32,
    pub shapes: Vec<Shape>,
}

/// One extruded letter from [`Geometry::text_glyphs`].
#[derive(Debug, Clone)]
pub struct Glyph {
    pub ch: char,
    /// The pen position of the glyph on the baseline.
    pub origin: Vec2,
    /// The horizontal distance from this glyph to the next.
    pub advance: f32,
    /// The letter in glyph space. The pen position is at the origin, the baseline is at
    /// Y = 0, and the letter faces +Z.
    pub geometry: Geometry,
}

/// The reason that an STL file cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StlError {
    /// The bytes are not a binary STL of the announced size, and not ASCII `facet` blocks.
    Invalid,
    /// The file has no triangles with an area.
    Empty,
}

impl std::fmt::Display for StlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StlError::Invalid => write!(f, "not a binary or ASCII STL file"),
            StlError::Empty => write!(f, "the STL file has no triangles"),
        }
    }
}

impl std::error::Error for StlError {}

/// Reads the normal and the corners of each triangle of a binary STL. A binary STL has an
/// 80 byte header, a triangle count, and 50 bytes for each triangle. Returns `None` when
/// the size does not agree with the count. An ASCII file can have the correct size only by
/// chance.
fn binary_stl(bytes: &[u8]) -> Option<Vec<(Vec3, [Vec3; 3])>> {
    let (count, records) = bytes.get(80..)?.split_first_chunk::<4>()?;
    let (records, rest) = records.as_chunks::<50>();
    if records.len() != u32::from_le_bytes(*count) as usize || !rest.is_empty() {
        return None;
    }
    let triangles = records.iter().map(|record| {
        // Twelve floats (the normal and three corners), then a 2 byte attribute.
        let floats = record.as_chunks::<4>().0;
        let v = |k: usize| {
            Vec3::from_array(std::array::from_fn(|i| {
                f32::from_le_bytes(floats[k * 3 + i])
            }))
        };
        (v(0), [v(1), v(2), v(3)])
    });
    Some(triangles.collect())
}

/// Reads the normal and the corners of each triangle of an ASCII STL.
fn ascii_stl(bytes: &[u8]) -> Result<Vec<(Vec3, [Vec3; 3])>, StlError> {
    let text = std::str::from_utf8(bytes).map_err(|_| StlError::Invalid)?;
    if !text.trim_start().starts_with("solid") {
        return Err(StlError::Invalid);
    }
    let mut triangles = Vec::new();
    let mut normal = Vec3::ZERO;
    let mut corners = Vec::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("facet") => {
                // `facet normal x y z`. A missing or bad number reads as 0.
                normal = Vec3::from_array(three_numbers(words.skip(1)).map(|c| c.unwrap_or(0.0)));
                corners.clear();
            }
            Some("vertex") => {
                let [Some(x), Some(y), Some(z)] = three_numbers(words) else {
                    return Err(StlError::Invalid);
                };
                corners.push(Vec3::new(x, y, z));
            }
            Some("endfacet") => {
                let tri = corners
                    .as_slice()
                    .try_into()
                    .map_err(|_| StlError::Invalid)?;
                triangles.push((normal, tri));
            }
            _ => {}
        }
    }
    Ok(triangles)
}

/// The next three words as numbers. A word that is not a number gives `None`.
fn three_numbers<'a>(mut words: impl Iterator<Item = &'a str>) -> [Option<f32>; 3] {
    let mut number = || words.next().and_then(|w| w.parse().ok());
    [number(), number(), number()]
}

/// The reason that text cannot be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextError {
    /// The bytes are not a TrueType or OpenType font.
    InvalidFont,
    /// The font has no glyph for this character.
    MissingGlyph(char),
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextError::InvalidFont => write!(f, "not a TrueType/OpenType font"),
            TextError::MissingGlyph(c) => write!(f, "the font has no glyph for {c:?}"),
        }
    }
}

impl std::error::Error for TextError {}

/// Collects the outline of a glyph as closed polygons. Curves become line segments.
#[derive(Default)]
struct OutlineCollector {
    /// World units per font unit.
    scale: f32,
    contours: Vec<Vec<Vec2>>,
    current: Vec<Vec2>,
    last: Vec2,
}

impl OutlineCollector {
    fn point(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new(x, y) * self.scale
    }

    /// Ends the current contour. It keeps the contour only when it has three points or more,
    /// because a smaller contour has no area.
    fn finish(&mut self) {
        if self.current.len() >= 3 {
            self.contours.push(std::mem::take(&mut self.current));
        }
        self.current.clear();
    }
}

impl fframes::ttf_parser::OutlineBuilder for OutlineCollector {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        self.line_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.last = self.point(x, y);
        self.current.push(self.last);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (p0, c, p1) = (self.last, self.point(x1, y1), self.point(x, y));
        let steps = 8;
        self.current.extend((1..=steps).map(|i| {
            let t = i as f32 / steps as f32;
            let u = 1.0 - t;
            p0 * (u * u) + c * (2.0 * u * t) + p1 * (t * t)
        }));
        self.last = p1;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (p0, c0, c1, p1) = (
            self.last,
            self.point(x1, y1),
            self.point(x2, y2),
            self.point(x, y),
        );
        let steps = 12;
        self.current.extend((1..=steps).map(|i| {
            let t = i as f32 / steps as f32;
            let u = 1.0 - t;
            p0 * (u * u * u) + c0 * (3.0 * u * u * t) + c1 * (3.0 * u * t * t) + p1 * (t * t * t)
        }));
        self.last = p1;
    }

    fn close(&mut self) {
        self.finish();
    }
}

/// True when `p` is inside `polygon` by the even-odd rule.
fn point_in_polygon(p: Vec2, polygon: &[Vec2]) -> bool {
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Sorts the contours of a glyph into outlines and holes by their winding. Fonts do not
/// agree on the direction (TrueType outlines go clockwise, CFF outlines counter-clockwise),
/// but the largest contour is always an outline. Contours that wind like it are outlines,
/// also where they overlap (variable fonts keep overlaps), the others are holes. A hole
/// goes into the smallest outline that contains it.
fn shapes_from_contours(contours: Vec<Vec<Vec2>>) -> Vec<Shape> {
    let areas: Vec<f32> = contours.iter().map(|c| signed_area(c)).collect();
    let Some(largest) = areas
        .iter()
        .copied()
        .max_by(|a, b| a.abs().total_cmp(&b.abs()))
    else {
        return Vec::new();
    };
    let mut shapes: Vec<(f32, Shape)> = Vec::new();
    let mut holes = Vec::new();
    for (contour, area) in contours.into_iter().zip(areas) {
        if (area > 0.0) == (largest > 0.0) {
            let shape = Shape {
                outer: contour,
                holes: Vec::new(),
            };
            shapes.push((area.abs(), shape));
        } else {
            holes.push(contour);
        }
    }
    for hole in holes {
        if let Some((_, shape)) = shapes
            .iter_mut()
            .filter(|(_, s)| point_in_polygon(hole[0], &s.outer))
            .min_by(|(a, _), (b, _)| a.total_cmp(b))
        {
            shape.holes.push(hole);
        }
    }
    shapes.into_iter().map(|(_, shape)| shape).collect()
}

/// Triangles that cover the area where the winding number of `contours` is not zero.
///
/// This is a scanline decomposition. Horizontal lines through each vertex and each edge
/// crossing cut the plane into slabs. In each slab, each span between edges with a winding
/// number that is not zero becomes a trapezoid. Ear clipping fails on holes, overlaps and
/// contours that cross themselves, which are frequent in fonts. This method does not.
fn fill_nonzero(contours: &[Vec<Vec2>]) -> Vec<[Vec2; 3]> {
    let edges: Vec<(Vec2, Vec2)> = contours
        .iter()
        .flat_map(|c| closed_edges(c))
        .filter(|(a, b)| (a.y - b.y).abs() > 1e-9)
        .collect();
    let mut ys: Vec<f32> = edges.iter().flat_map(|(a, b)| [a.y, b.y]).collect();
    for i in 0..edges.len() {
        for j in i + 1..edges.len() {
            if let Some(y) = crossing_y(edges[i], edges[j]) {
                ys.push(y);
            }
        }
    }
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 1e-7);

    let x_at = |(a, b): (Vec2, Vec2), y: f32| a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y);
    let mut out = Vec::new();
    // The edges in the current slab: X at the middle, bottom and top, and the winding.
    let mut active: Vec<(f32, f32, f32, i32)> = Vec::new();
    for slab in ys.windows(2) {
        let (y0, y1) = (slab[0], slab[1]);
        let ym = f32::midpoint(y0, y1);
        active.clear();
        for &edge in &edges {
            let (a, b) = edge;
            if (a.y.min(b.y)..a.y.max(b.y)).contains(&ym) {
                let winding = if b.y > a.y { 1 } else { -1 };
                active.push((x_at(edge, ym), x_at(edge, y0), x_at(edge, y1), winding));
            }
        }
        active.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut winding = 0;
        let mut left: Option<(f32, f32)> = None;
        for &(_, x0, x1, w) in &active {
            let before = winding;
            winding += w;
            if before == 0 && winding != 0 {
                left = Some((x0, x1));
            } else if before != 0
                && winding == 0
                && let Some((l0, l1)) = left.take()
            {
                let (bl, br, tr, tl) = (
                    Vec2::new(l0, y0),
                    Vec2::new(x0, y0),
                    Vec2::new(x1, y1),
                    Vec2::new(l1, y1),
                );
                if (br.x - bl.x) > 1e-9 {
                    out.push([bl, br, tr]);
                }
                if (tr.x - tl.x) > 1e-9 {
                    out.push([bl, tr, tl]);
                }
            }
        }
    }
    out
}

/// The height where two segments cross, or `None` when they do not cross.
fn crossing_y((p1, p2): (Vec2, Vec2), (q1, q2): (Vec2, Vec2)) -> Option<f32> {
    let r = p2 - p1;
    let s = q2 - q1;
    let denom = r.perp_dot(s);
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = (q1 - p1).perp_dot(s) / denom;
    let u = (q1 - p1).perp_dot(r) / denom;
    (t > 1e-6 && t < 1.0 - 1e-6 && u > 1e-6 && u < 1.0 - 1e-6).then_some(p1.y + r.y * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks that each triangle winds counter-clockwise around its vertex normals.
    fn assert_outward(geometry: &Geometry) {
        let mut agree = 0;
        let mut total = 0;
        for tri in geometry.indices.as_chunks::<3>().0 {
            let [a, b, c] = tri.map(|i| i as usize);
            let face = face_normal([a, b, c].map(|i| geometry.positions[i]));
            if face.length_squared() < 1e-12 {
                continue;
            }
            total += 1;
            let n = geometry.normals[a] + geometry.normals[b] + geometry.normals[c];
            if face.dot(n) > 0.0 {
                agree += 1;
            }
        }
        assert!(total > 0);
        assert_eq!(
            agree, total,
            "{agree} of {total} triangles face their normals"
        );
    }

    fn cube_soup() -> Vec<[Vec3; 3]> {
        let cube = Geometry::cuboid(2.0, 2.0, 2.0);
        cube.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| t.map(|i| cube.positions[i as usize]))
            .collect()
    }

    #[test]
    fn reads_binary_and_ascii_stl() {
        let soup = cube_soup();
        let mut binary = vec![0u8; 80];
        binary.extend_from_slice(&(soup.len() as u32).to_le_bytes());
        let mut ascii = String::from("solid cube\n");
        for (i, tri) in soup.iter().enumerate() {
            let n = face_normal(*tri).normalize();
            // Half of the triangles have the wrong winding. The normal must correct them.
            let tri = if i % 2 == 0 {
                *tri
            } else {
                [tri[0], tri[2], tri[1]]
            };
            for v in [n, tri[0], tri[1], tri[2]] {
                for c in v.to_array() {
                    binary.extend_from_slice(&c.to_le_bytes());
                }
            }
            binary.extend_from_slice(&[0, 0]);
            use std::fmt::Write;
            writeln!(
                ascii,
                "  facet normal {} {} {}\n    outer loop",
                n.x, n.y, n.z
            )
            .unwrap();
            for v in tri {
                writeln!(ascii, "      vertex {} {} {}", v.x, v.y, v.z).unwrap();
            }
            ascii += "    endloop\n  endfacet\n";
        }
        ascii += "endsolid cube\n";
        for geometry in [
            Geometry::from_stl(&binary).unwrap(),
            Geometry::from_stl(ascii.as_bytes()).unwrap(),
        ] {
            assert_eq!(geometry.triangle_count(), 12);
            assert_eq!(geometry.vertex_count(), 36);
            assert_eq!(geometry.uvs, Vec::<Vec2>::new());
            assert_outward(&geometry);
            assert_eq!(geometry.bounds().max, Vec3::splat(1.0));
            // A cube stays faceted: 24 vertices at 8 positions.
            let smoothed = geometry.smooth(30.0);
            assert_eq!(smoothed.vertex_count(), 24);
            assert_outward(&smoothed);
        }
        assert_eq!(
            Geometry::from_stl(b"not a model").err(),
            Some(StlError::Invalid)
        );
        assert_eq!(
            Geometry::from_stl(b"solid x\nendsolid x\n").err(),
            Some(StlError::Empty)
        );
    }

    #[test]
    fn smooth_rounds_a_faceted_sphere() {
        let sphere = Geometry::sphere(1.0, 24, 16).flat();
        let faceted = sphere.vertex_count();
        let smoothed = sphere.smooth(60.0);
        assert!(smoothed.vertex_count() < faceted / 4);
        for (p, n) in smoothed.positions.iter().zip(&smoothed.normals) {
            assert!(p.normalize().dot(*n) > 0.98, "{p} {n}");
        }
    }

    #[test]
    fn primitives_wind_outward() {
        assert_outward(&Geometry::cuboid(1.0, 2.0, 3.0));
        assert_outward(&Geometry::rounded_box(1.0, 2.0, 3.0, 0.2, 3));
        assert_outward(&Geometry::sphere(1.0, 16, 8));
        assert_outward(&Geometry::cylinder(0.5, 1.0, 2.0, 16));
        assert_outward(&Geometry::cone(1.0, 2.0, 16));
        assert_outward(&Geometry::capsule(0.5, 1.0, 4, 16));
        assert_outward(&Geometry::torus(1.0, 0.3, 8, 16));
        assert_outward(&Geometry::plane(1.0, 1.0));
        assert_outward(&Geometry::floor(1.0, 1.0));
        assert_outward(&Geometry::icosahedron(1.0, 1));
        assert_outward(&Geometry::extrude(&Geometry::polygon(1.0, 6), 0.5, 0.1));
        assert_outward(&Geometry::extrude(
            &[
                Vec2::new(-1.0, -0.5),
                Vec2::new(1.0, -0.5),
                Vec2::new(1.0, 0.5),
                Vec2::new(-1.0, 0.5),
            ],
            0.3,
            0.0,
        ));
        assert_outward(&Geometry::tube(
            &[Vec3::ZERO, Vec3::X, Vec3::new(1.0, 1.0, 0.0)],
            0.1,
            8,
        ));
        assert_outward(&Geometry::cuboid(1.0, 1.0, 1.0).scale(-1.0, 1.0, 1.0));
    }

    #[test]
    fn rounded_box_keeps_its_size() {
        let b = Geometry::rounded_box(2.0, 1.0, 0.5, 0.1, 4).bounds();
        assert!((b.size() - Vec3::new(2.0, 1.0, 0.5)).abs().max_element() < 1e-4);
    }

    /// A size that an animation drives below zero must not panic in a frame.
    #[test]
    fn negative_sizes_do_not_panic() {
        let b = Geometry::cuboid(-1.0, 0.5, 0.25).bounds();
        assert!((b.size() - Vec3::new(1.0, 0.5, 0.25)).abs().max_element() < 1e-4);
        let _ = Geometry::extrude(&Geometry::polygon(-1.0, 6), -0.2, 0.05);
    }

    #[test]
    fn concave_shapes_fill() {
        let l_shape = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(2.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 2.0),
            Vec2::new(0.0, 2.0),
        ];
        let area: f32 = fill_nonzero(&[l_shape])
            .iter()
            .map(|[a, b, c]| (*b - *a).perp_dot(*c - *a) * 0.5)
            .sum();
        assert!((area - 3.0).abs() < 1e-5);
    }

    /// The front cap of each glyph covers the outline once. No triangles overlap, and the
    /// holes stay open.
    #[test]
    fn caps_cover_exactly_the_outline() {
        let Ok(font) = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../examples/conference-splash-screen/media/Inter_18pt-ExtraBold.ttf"
        )) else {
            return;
        };
        for g in Geometry::text_shapes(&font, "fframes aeo8B@%&gQR", 1.0, 0.0).unwrap() {
            for shape in &g.shapes {
                let expected = signed_area(&shape.outer).abs()
                    - shape
                        .holes
                        .iter()
                        .map(|h| signed_area(h).abs())
                        .sum::<f32>();
                let geometry = Geometry::extrude_shape(shape, 0.2, 0.0, 1);
                let (mut signed, mut abs) = (0.0, 0.0);
                for t in geometry.indices.as_chunks::<3>().0 {
                    let [a, b, c] = t.map(|i| i as usize);
                    if [a, b, c].iter().all(|&i| geometry.normals[i] == Vec3::Z) {
                        let z = face_normal([a, b, c].map(|i| geometry.positions[i])).z * 0.5;
                        signed += z;
                        abs += z.abs();
                    }
                }
                // No triangle overlaps another or faces the wrong way. The plain polygon
                // area counts overlaps of font contours two times, so it is only a limit.
                assert!(
                    (signed - abs).abs() < 1e-4,
                    "{:?}: signed {signed}, abs {abs}",
                    g.ch
                );
                assert!(
                    signed <= expected + 1e-3 && signed > expected * 0.8,
                    "{:?}: expected {expected}, got {signed}",
                    g.ch
                );
            }
        }
    }

    /// Profiles without a segment give an empty geometry, not a panic.
    #[test]
    fn short_lathe_profiles_are_empty() {
        assert_eq!(Geometry::lathe(&[], 16).vertex_count(), 0);
        assert_eq!(Geometry::lathe(&[Vec2::ONE], 16).vertex_count(), 0);
    }

    /// The subdivision of three.js: `(detail + 1)²` triangles per face, and no triangle
    /// samples across the U seam.
    #[test]
    fn icosahedron_subdivides_like_three_js() {
        for detail in [0, 1, 2, 5] {
            let g = Geometry::icosahedron(1.0, detail);
            assert_eq!(g.triangle_count(), 20 * (detail as usize + 1).pow(2));
            assert_outward(&g);
            // Faces near the poles cover a wide azimuth, and so do the large faces of
            // detail 0 and 1, as in three.js. The others must not reach across the seam.
            let away_from_poles =
                |t: &&[u32; 3]| t.iter().all(|&i| g.positions[i as usize].y.abs() < 0.8);
            for t in (g.indices.as_chunks::<3>().0.iter())
                .filter(away_from_poles)
                .filter(|_| detail >= 2)
            {
                let us = t.map(|i| g.uvs[i as usize].x);
                let span = us.iter().fold(f32::MIN, |a, &b| a.max(b))
                    - us.iter().fold(f32::MAX, |a, &b| a.min(b));
                assert!(span < 0.25, "detail {detail}: U span {span}");
            }
            for p in &g.positions {
                assert!((p.length() - 1.0).abs() < 1e-5);
            }
        }
    }

    /// V runs down the image: the top of a texture is at the top of a sphere, a cylinder
    /// and a lathe, as in three.js.
    #[test]
    fn textures_are_upright() {
        let top_v = |g: &Geometry| {
            let i = (0..g.vertex_count())
                .filter(|&i| g.normals[i].y.abs() < 0.9)
                .max_by(|&a, &b| g.positions[a].y.total_cmp(&g.positions[b].y))
                .unwrap();
            g.uvs[i].y
        };
        // the highest ring with a sideways normal is 2 of 8 rows down
        assert!(top_v(&Geometry::sphere(1.0, 16, 8)) <= 0.25 + 1e-6);
        assert!(top_v(&Geometry::cylinder(1.0, 1.0, 2.0, 16)) < 1e-6);
        assert!(
            top_v(&Geometry::lathe(
                &[Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)],
                16
            )) < 1e-6
        );
        // the side of a cylinder uses the whole texture
        let side = Geometry::cylinder(1.0, 1.0, 2.0, 16);
        let vs: Vec<f32> = (0..side.vertex_count())
            .filter(|&i| side.normals[i].y.abs() < 0.5)
            .map(|i| side.uvs[i].y)
            .collect();
        assert_eq!(vs.iter().fold(f32::MAX, |a, &b| a.min(b)), 0.0);
        assert_eq!(vs.iter().fold(f32::MIN, |a, &b| a.max(b)), 1.0);
    }

    #[test]
    fn out_of_range_indices_are_dropped() {
        let g = Geometry::new(
            vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            Vec::new(),
            Vec::new(),
            vec![0, 1, 2, 0, 1, 7, 2],
        );
        assert_eq!(g.indices, vec![0, 1, 2]);
        let _ = g.clone().flat().smooth(30.0);
    }

    /// Variable fonts keep overlapping contours. Both directions of font outlines must
    /// fill their union, without turning the overlap into a hole.
    #[test]
    fn overlapping_contours_stay_solid() {
        let square = |o: f32| {
            vec![
                Vec2::new(o, o),
                Vec2::new(o + 2.0, o),
                Vec2::new(o + 2.0, o + 2.0),
                Vec2::new(o, o + 2.0),
            ]
        };
        let hole = vec![
            Vec2::new(0.5, 0.5),
            Vec2::new(0.5, 0.8),
            Vec2::new(0.8, 0.8),
            Vec2::new(0.8, 0.5),
        ];
        for flip in [false, true] {
            let mut contours = vec![square(0.0), square(1.0), hole.clone()];
            if flip {
                for c in &mut contours {
                    c.reverse();
                }
            }
            let shapes = shapes_from_contours(contours);
            assert_eq!(shapes.len(), 2, "flip {flip}");
            assert_eq!(shapes.iter().map(|s| s.holes.len()).sum::<usize>(), 1);
        }
    }

    /// Welding keeps the U seam of a sphere: no triangle samples across it.
    #[test]
    fn smooth_keeps_uv_seams() {
        let g = Geometry::sphere(1.0, 16, 8).flat().smooth(60.0);
        for t in g.indices.as_chunks::<3>().0 {
            let us = t.map(|i| g.uvs[i as usize].x);
            let span = us.iter().fold(f32::MIN, |a, &b| a.max(b))
                - us.iter().fold(f32::MAX, |a, &b| a.min(b));
            assert!(span < 0.5, "U span {span}");
        }
    }

    #[test]
    fn transform_moves_morph_targets() {
        let mut g = Geometry::plane(1.0, 1.0);
        g.morph_targets.push(MorphTarget {
            name: None,
            positions: vec![Vec3::X; 4],
            normals: Vec::new(),
        });
        let g = g.rotate_z(FRAC_PI_2).translate(5.0, 0.0, 0.0);
        let d = g.morph_targets[0].positions[0];
        assert!((d - Vec3::Y).length() < 1e-5, "{d}");
    }

    #[test]
    fn degenerate_sizes_give_finite_uvs() {
        for g in [Geometry::cuboid(0.0, 1.0, 1.0), Geometry::circle(0.0, 8)] {
            assert!(g.uvs.iter().all(|uv| uv.is_finite()));
        }
    }
}
