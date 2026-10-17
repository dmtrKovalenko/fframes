//! A six axis industrial arm with a camera eye on its gripper, and its inverse kinematics.
//! The three wrist axes meet in one point, so the inverse kinematics has an analytic solution.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use fframes_3d::*;

/// Height of the J1 turret above the floor.
pub const BASE_H: f32 = 0.45;
/// Height of J2 above J1.
pub const J2_UP: f32 = 0.42;
/// Distance of J2 in front of the J1 axis.
pub const A1: f32 = 0.16;
/// Length of the upper arm, from J2 to J3.
pub const L2: f32 = 1.05;
/// Height of the forearm axis above J3.
pub const A3: f32 = 0.12;
/// Length of the forearm, from J3 to J5.
pub const L3: f32 = 1.0;
/// Distance of the J4 roll joint from J3 along the forearm.
pub const J4_AT: f32 = 0.62;
/// Distance from J5 to J6 along the tool.
pub const J6_OFFSET: f32 = 0.15;
/// Distance from J6 to the point between the finger tips.
pub const GRIP: f32 = 0.24;
/// Distance from J5 to the grip point.
pub const TIP: f32 = J6_OFFSET + GRIP;
/// Half the finger gap of the open gripper.
pub const OPEN: f32 = 0.12;
/// Position of the eye lens in the J6 frame.
const EYE: Vec3 = Vec3::new(0.0, 0.068, 0.1);

/// Joint angles in radians: turret (J1), shoulder (J2), elbow (J3), forearm roll (J4),
/// wrist (J5) and flange roll (J6).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Joints {
    pub q: [f32; 6],
}

impl Joints {
    pub fn lerp(&self, other: &Joints, t: f32) -> Joints {
        Joints {
            q: std::array::from_fn(|i| self.q[i] + (other.q[i] - self.q[i]) * t),
        }
    }

    pub fn add(&self, offsets: [f32; 6]) -> Joints {
        Joints {
            q: std::array::from_fn(|i| self.q[i] + offsets[i]),
        }
    }
}

/// Local transforms of the six joint nodes.
fn chain(q: &Joints) -> [Mat4; 6] {
    [
        Mat4::from_translation(vec3(0.0, BASE_H, 0.0)) * Mat4::from_rotation_y(q.q[0]),
        Mat4::from_translation(vec3(0.0, J2_UP, A1)) * Mat4::from_rotation_x(q.q[1]),
        Mat4::from_translation(vec3(0.0, L2, 0.0)) * Mat4::from_rotation_x(q.q[2]),
        Mat4::from_translation(vec3(0.0, A3, J4_AT)) * Mat4::from_rotation_z(q.q[3]),
        Mat4::from_translation(vec3(0.0, 0.0, L3 - J4_AT)) * Mat4::from_rotation_x(q.q[4]),
        Mat4::from_translation(vec3(0.0, 0.0, J6_OFFSET)) * Mat4::from_rotation_z(q.q[5]),
    ]
}

/// World transform of the J6 flange.
pub fn flange(q: &Joints) -> Mat4 {
    chain(q).iter().fold(Mat4::IDENTITY, |m, j| m * *j)
}

/// World transform of the grip point. The tool axis is +Z and the fingers close along +X.
pub fn tip(q: &Joints) -> Mat4 {
    flange(q) * Mat4::from_translation(vec3(0.0, 0.0, GRIP))
}

/// World position of the eye lens.
pub fn eye(q: &Joints) -> Vec3 {
    flange(q).transform_point3(EYE)
}

/// The angle `a + k * TAU` closest to `near`.
fn unwrap(a: f32, near: f32) -> f32 {
    a + ((near - a) / TAU).round() * TAU
}

/// Returns the joint angles that put the grip point at `position` with the tool frame
/// `rotation`. The tool axis is local +Z and the finger axis is local +X. Of the two wrist
/// solutions, it uses the one nearest to `hint`. It unwraps J1, J4 and J6 to the turn of `hint`.
pub fn solve(position: Vec3, rotation: Quat, hint: &Joints) -> Joints {
    let axis = rotation * Vec3::Z;
    let wrist = position - axis * TIP;
    let q1 = unwrap(wrist.x.atan2(wrist.z), hint.q[0]);
    let r = (wrist.x * wrist.x + wrist.z * wrist.z).sqrt() - A1;
    let h = wrist.y - (BASE_H + J2_UP);
    let l3 = (L3 * L3 + A3 * A3).sqrt();
    let phi = A3.atan2(L3);
    let d = (r * r + h * h).sqrt().clamp(0.05, L2 + l3 - 1e-3);
    let gamma = h.atan2(r);
    let psi = ((L2 * L2 + d * d - l3 * l3) / (2.0 * L2 * d))
        .clamp(-1.0, 1.0)
        .acos();
    // Elbow up.
    let theta2 = gamma + psi;
    let elbow = Vec2::new(L2 * theta2.cos(), L2 * theta2.sin());
    let theta3 = (h - elbow.y).atan2(r - elbow.x) - phi;
    let q2 = FRAC_PI_2 - theta2;
    let q3 = -theta3 - q2;

    // The wrist rotation is M = Rz(q4) Rx(q5) Rz(q6).
    let r3 = Quat::from_rotation_y(q1) * Quat::from_rotation_x(q2 + q3);
    let m = r3.inverse() * rotation;
    let z = m * Vec3::Z;
    let b = z.z.clamp(-1.0, 1.0).acos();
    let candidates: Vec<(f32, f32)> = if b.sin().abs() < 1e-4 {
        vec![(hint.q[3], if z.z > 0.0 { 0.0 } else { PI })]
    } else {
        let a = z.x.atan2(-z.y);
        vec![(a, b), (a + PI, -b)]
    };
    let mut best: Option<(f32, Joints)> = None;
    for (a, b) in candidates {
        let a = unwrap(a, hint.q[3]);
        let n = (Quat::from_rotation_z(a) * Quat::from_rotation_x(b)).inverse() * m;
        let x = n * Vec3::X;
        let c = unwrap(x.y.atan2(x.x), hint.q[5]);
        let joints = Joints {
            q: [q1, q2, q3, a, b, c],
        };
        let cost =
            (a - hint.q[3]).abs() + 0.5 * (c - hint.q[5]).abs() + 0.3 * (b - hint.q[4]).abs();
        if best.as_ref().is_none_or(|(best_cost, _)| cost < *best_cost) {
            best = Some((cost, joints));
        }
    }
    best.map(|(_, j)| j).expect("a wrist solution")
}

/// The tool points straight down and the fingers close along the horizontal `fingers`.
pub fn tool_down(fingers: Vec3) -> Quat {
    frame(fingers, Vec3::NEG_Y)
}

/// A rotation with +Z along `z` and +X along `x`, made orthogonal to `z`.
pub fn frame(x: Vec3, z: Vec3) -> Quat {
    let z = z.normalize();
    let y = z.cross(x).normalize();
    let x = y.cross(z);
    Quat::from_mat3(&Mat3::from_cols(x, y, z))
}

/// The tool and the eye point along `look` with level fingers, turned by `roll` radians.
pub fn looking(look: Vec3, roll: f32) -> Quat {
    let z = look.normalize();
    let side = Vec3::Y.cross(z).normalize_or(Vec3::X);
    Quat::from_axis_angle(z, roll) * frame(side, z)
}

/// The scene nodes that [`Arm::apply`] poses.
pub struct Arm {
    joints: [NodeId; 6],
    fingers: [NodeId; 2],
    spring_body: NodeId,
    spring_rod: NodeId,
    status: NodeId,
    ring: NodeId,
    iris: NodeId,
    lid: NodeId,
    /// The spot light of the eye and its beam in the air.
    eye_light: NodeId,
    beam: NodeId,
    /// The light of the base ring on the floor.
    ring_light: NodeId,
}

/// The color of the eye light.
const EYE_LIGHT: u32 = 0xffd6a8;

/// The lights and the eye of the robot.
#[derive(Debug, Clone, Copy)]
pub struct Expression {
    /// Glow of the base ring.
    pub ring: f32,
    /// Glow of the gripper status light.
    pub status: f32,
    /// Glow of the iris.
    pub eye: f32,
    /// Eye lid: 1 is open, 0 is closed.
    pub lid: f32,
    /// How much the beam of the eye shows in the air: 1 in the dark lab, 0 when it is lit.
    pub beam: f32,
}

/// The materials of the arm.
pub struct Palette {
    pub white: Material,
    pub graphite: Material,
    pub chrome: Material,
    pub rubber: Material,
    pub dark: Material,
    pub yellow: Material,
    pub glass: Material,
}

impl Palette {
    pub fn new() -> Self {
        Self {
            white: Material::standard(0xeeeeea).roughness(0.36).clearcoat(0.7),
            graphite: Material::metal(0x2c2d31, 0.38),
            chrome: Material::metal(0xe0e3e7, 0.1),
            rubber: Material::standard(0x151517).roughness(0.85),
            dark: Material::metal(0x18191b, 0.55),
            yellow: Material::standard(0xf2bd00).roughness(0.45),
            glass: Material::standard(0x05070a)
                .roughness(0.03)
                .clearcoat(1.0)
                .env_intensity(1.6),
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::new()
    }
}

/// The convex hull of circles as a polygon. It gives cast parts with round ends.
pub fn hull(circles: &[(Vec2, f32)]) -> Vec<Vec2> {
    let mut points = Vec::new();
    for &(c, r) in circles {
        for i in 0..48 {
            let a = i as f32 / 48.0 * TAU;
            points.push(c + Vec2::new(a.cos(), a.sin()) * r);
        }
    }
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    let mut lower: Vec<Vec2> = Vec::new();
    for &p in &points {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Vec2> = Vec::new();
    for &p in points.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// A plate in the side plane of the arm. `shape` is in (z, y), `thickness` is along X and the
/// center is at `x`.
fn side_plate(shape: &[Vec2], thickness: f32, bevel: f32, x: f32) -> Geometry {
    let flipped: Vec<Vec2> = shape.iter().map(|p| Vec2::new(-p.x, p.y)).collect();
    Geometry::extrude(&flipped, thickness, bevel)
        .rotate_y(deg(90.0))
        .translate(x, 0.0, 0.0)
}

/// A cylinder along X.
fn axle(radius: f32, length: f32, x: f32, y: f32, z: f32) -> Geometry {
    Geometry::cylinder(radius, radius, length, 48)
        .rotate_z(deg(90.0))
        .translate(x, y, z)
}

/// A cylinder along the unit vector `axis`, centered at `center`.
fn rod(radius: f32, length: f32, center: Vec3, axis: Vec3, segments: u32) -> Geometry {
    Geometry::cylinder(radius, radius, length, segments)
        .rotate(Quat::from_rotation_arc(Vec3::Y, axis))
        .translate(center.x, center.y, center.z)
}

/// Hex bolt heads with washers on a circle. The plane of the circle faces `normal`.
fn bolt_circle(count: u32, radius: f32, bolt: f32, center: Vec3, normal: Vec3) -> Geometry {
    let head = Geometry::merge([
        Geometry::cylinder(bolt * 1.35, bolt * 1.35, bolt * 0.25, 16).translate(
            0.0,
            bolt * 0.12,
            0.0,
        ),
        Geometry::cylinder(bolt, bolt, bolt * 0.8, 6).translate(0.0, bolt * 0.62, 0.0),
    ]);
    let to_normal = Quat::from_rotation_arc(Vec3::Y, normal);
    Geometry::merge((0..count).map(|i| {
        let a = i as f32 / count as f32 * TAU;
        head.clone()
            .translate(a.cos() * radius, 0.0, a.sin() * radius)
            .rotate(to_normal)
            .translate(center.x, center.y, center.z)
    }))
}

/// Cooling fins: thin rings from `from` to `to`.
fn fins(count: u32, radius: f32, from: Vec3, to: Vec3) -> Geometry {
    let axis = (to - from).normalize_or(Vec3::Y);
    let ring = Geometry::torus(radius, 0.008, 6, 40).rotate_x(deg(90.0));
    let to_axis = Quat::from_rotation_arc(Vec3::Y, axis);
    Geometry::merge((0..count).map(|i| {
        let p = from.lerp(to, (i as f32 + 0.5) / count as f32);
        ring.clone().rotate(to_axis).translate(p.x, p.y, p.z)
    }))
}

/// A corrugated hose: a tube with rings along `path`.
fn conduit(path: &[Vec3], radius: f32) -> (Geometry, Geometry) {
    let tube = Geometry::tube(path, radius, 14);
    let mut rings = Vec::new();
    let mut along = 0.0f32;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        let dir = (b - a).normalize_or(Vec3::Y);
        let mut s = (0.035 - along).max(0.0);
        while s < len {
            let p = a + dir * s;
            rings.push(
                Geometry::torus(radius * 1.02, radius * 0.18, 6, 18)
                    .rotate_x(deg(90.0))
                    .rotate(Quat::from_rotation_arc(Vec3::Y, dir))
                    .translate(p.x, p.y, p.z),
            );
            s += 0.035;
        }
        along = s - len;
    }
    (tube, Geometry::merge(rings))
}

/// A warning label at `at` that faces `normal` (X or Z). It is a yellow triangle with a black
/// border and an exclamation mark.
fn warning(scene: &mut Scene, parent: NodeId, p: &Palette, at: Vec3, normal: Vec3, size: f32) {
    let tri = Geometry::polygon(size, 3);
    let to_normal = Quat::from_rotation_arc(Vec3::Z, normal);
    let place = |g: Geometry, out: f32| {
        g.rotate(to_normal).translate(
            at.x + normal.x * out,
            at.y + normal.y * out,
            at.z + normal.z * out,
        )
    };
    scene.add_to(
        parent,
        Node::mesh(
            place(Geometry::extrude(&tri, 0.003, 0.0), 0.0),
            p.dark.clone(),
        ),
    );
    scene.add_to(
        parent,
        Node::mesh(
            place(
                Geometry::extrude(&Geometry::polygon(size * 0.78, 3), 0.003, 0.0).translate(
                    0.0,
                    -size * 0.04,
                    0.0,
                ),
                0.002,
            ),
            p.yellow.clone(),
        ),
    );
    let mark = Geometry::merge([
        Geometry::rounded_box(size * 0.12, size * 0.42, 0.003, size * 0.03, 1).translate(
            0.0,
            size * 0.02,
            0.0,
        ),
        Geometry::cylinder(size * 0.07, size * 0.07, 0.003, 12)
            .rotate_x(deg(90.0))
            .translate(0.0, -size * 0.32, 0.0),
    ]);
    scene.add_to(parent, Node::mesh(place(mark, 0.004), p.dark.clone()));
}

/// Raised lettering, turned by `rotation` and centered on `at`. It adds nothing when the
/// font cannot make the text.
#[allow(clippy::too_many_arguments)]
fn lettering(
    scene: &mut Scene,
    parent: NodeId,
    material: &Material,
    font: &[u8],
    text: &str,
    size: f32,
    at: Vec3,
    rotation: Quat,
) {
    if let Ok(g) = Geometry::text(font, text, size, 0.006, 0.0012) {
        let c = g.bounds().center();
        scene.add_to(
            parent,
            Node::mesh(
                g.translate(-c.x, -c.y, 0.0)
                    .rotate(rotation)
                    .translate(at.x, at.y, at.z),
                material.clone(),
            ),
        );
    }
}

/// A motor along `axis` from `at`: body, fins, end cap with bolts and a connector.
fn motor(
    scene: &mut Scene,
    parent: NodeId,
    p: &Palette,
    at: Vec3,
    axis: Vec3,
    radius: f32,
    length: f32,
) {
    let to_axis = Quat::from_rotation_arc(Vec3::Y, axis);
    let center = at + axis * (length * 0.5);
    scene.add_to(
        parent,
        Node::mesh(
            Geometry::cylinder(radius, radius, length, 48)
                .rotate(to_axis)
                .translate(center.x, center.y, center.z),
            p.white.clone(),
        ),
    );
    scene.add_to(
        parent,
        Node::mesh(
            fins(
                7,
                radius + 0.004,
                at + axis * (length * 0.15),
                at + axis * (length * 0.75),
            ),
            p.graphite.clone(),
        ),
    );
    let cap_at = at + axis * (length + 0.015);
    scene.add_to(
        parent,
        Node::mesh(
            Geometry::cylinder(radius * 0.92, radius * 0.92, 0.03, 48)
                .rotate(to_axis)
                .translate(cap_at.x, cap_at.y, cap_at.z),
            p.graphite.clone(),
        ),
    );
    scene.add_to(
        parent,
        Node::mesh(
            bolt_circle(6, radius * 0.7, 0.008, at + axis * (length + 0.03), axis),
            p.chrome.clone(),
        ),
    );
    let side = if axis.y.abs() > 0.5 { Vec3::Z } else { Vec3::Y };
    let plug_at = at + axis * (length * 0.85) + side * radius;
    scene.add_to(
        parent,
        Node::mesh(
            Geometry::rounded_box(0.05, 0.05, 0.05, 0.012, 2)
                .translate(plug_at.x, plug_at.y, plug_at.z),
            p.dark.clone(),
        ),
    );
    let nut = at + axis * (length * 0.85) + side * (radius + 0.035);
    scene.add_to(
        parent,
        Node::mesh(rod(0.016, 0.03, nut, side, 6), p.chrome.clone()),
    );
}

impl Arm {
    pub fn build(scene: &mut Scene, p: &Palette, font: &[u8]) -> Arm {
        // ---- The base, bolted to the floor.
        let base = scene.add(Node::group().named("base"));
        scene.add_to(
            base,
            Node::mesh(
                Geometry::rounded_box(1.0, 0.05, 1.0, 0.02, 3).translate(0.0, 0.025, 0.0),
                p.graphite.clone(),
            ),
        );
        let anchors = Geometry::merge(
            [
                (-1.0f32, -1.0f32),
                (1.0, -1.0),
                (-1.0, 1.0),
                (1.0, 1.0),
                (0.0, -1.0),
                (0.0, 1.0),
                (-1.0, 0.0),
                (1.0, 0.0),
            ]
            .iter()
            .map(|(x, z)| bolt_circle(1, 0.0, 0.026, vec3(x * 0.43, 0.05, z * 0.43), Vec3::Y)),
        );
        scene.add_to(base, Node::mesh(anchors, p.chrome.clone()));
        let column = [
            vec2(0.0, 0.05),
            vec2(0.36, 0.05),
            vec2(0.36, 0.09),
            vec2(0.31, 0.13),
            vec2(0.3, 0.37),
            vec2(0.335, 0.405),
            vec2(0.335, 0.45),
            vec2(0.0, 0.45),
        ];
        scene.add_to(
            base,
            Node::mesh(Geometry::lathe(&column, 96), p.white.clone()),
        );
        let ribs = Geometry::merge((0..28).map(|i| {
            let a = i as f32 / 28.0 * TAU;
            Geometry::rounded_box(0.014, 0.2, 0.045, 0.005, 1)
                .translate(0.0, 0.25, 0.3)
                .rotate_y(a)
        }));
        scene.add_to(base, Node::mesh(ribs, p.graphite.clone()));
        let ring = scene.add_to(
            base,
            Node::mesh(
                Geometry::torus(0.336, 0.008, 8, 128)
                    .rotate_x(deg(90.0))
                    .translate(0.0, 0.385, 0.0),
                Material::standard(0x111111).emissive(0xff4a00, 0.0),
            ),
        );
        // The ring tints the floor around the base. Point lights cast no shadows, so the
        // light passes through the column.
        let ring_light = scene.add_to(
            base,
            Node::light(Light::point(0xff4a00, 0.0, 1.6)).at(0.0, 0.385, 0.0),
        );
        // The cable goes through a drag chain into a box on the floor.
        scene.add_to(
            base,
            Node::mesh(
                Geometry::rounded_box(0.18, 0.14, 0.1, 0.02, 2).translate(0.0, 0.2, -0.32),
                p.dark.clone(),
            ),
        );
        let chain_path: Vec<Vec3> = (0..=40)
            .map(|i| {
                let t = i as f32 / 40.0;
                let a = t * PI;
                vec3(
                    -0.9 * t,
                    0.06 + 0.16 * a.sin() * (1.0 - t * 0.5),
                    -0.45 - 0.55 * t,
                )
            })
            .collect();
        let links = Geometry::merge(
            chain_path
                .windows(2)
                .enumerate()
                .filter(|(i, _)| i % 2 == 0)
                .map(|(_, w)| {
                    let dir = (w[1] - w[0]).normalize_or(Vec3::X);
                    let mid = (w[0] + w[1]) * 0.5;
                    Geometry::rounded_box(0.09, 0.05, 0.06, 0.01, 1)
                        .rotate(Quat::from_rotation_arc(Vec3::Z, dir))
                        .translate(mid.x, mid.y, mid.z)
                }),
        );
        scene.add_to(base, Node::mesh(links, p.dark.clone()));
        scene.add_to(
            base,
            Node::mesh(
                Geometry::rounded_box(0.32, 0.16, 0.24, 0.03, 2).translate(-0.98, 0.08, -1.02),
                p.graphite.clone(),
            ),
        );
        scene.add_to(
            base,
            Node::mesh(
                Geometry::rounded_box(0.2, 0.004, 0.12, 0.002, 1).translate(-0.98, 0.162, -1.02),
                p.yellow.clone(),
            ),
        );

        // ---- J1: the turret.
        let j1 = scene.add_to(base, Node::group());
        scene.add_to(
            j1,
            Node::mesh(
                Geometry::cylinder(0.34, 0.345, 0.07, 96).translate(0.0, 0.035, 0.0),
                p.graphite.clone(),
            ),
        );
        scene.add_to(
            j1,
            Node::mesh(
                bolt_circle(16, 0.3, 0.011, vec3(0.0, 0.07, 0.0), Vec3::Y),
                p.chrome.clone(),
            ),
        );
        let cheek = hull(&[
            (vec2(A1, J2_UP), 0.235),
            (vec2(-0.08, 0.13), 0.22),
            (vec2(0.18, 0.1), 0.2),
        ]);
        let inner = hull(&[(vec2(A1, J2_UP), 0.17), (vec2(-0.04, 0.16), 0.15)]);
        for x in [-0.25f32, 0.25] {
            scene.add_to(
                j1,
                Node::mesh(side_plate(&cheek, 0.09, 0.025, x), p.white.clone()),
            );
            scene.add_to(
                j1,
                Node::mesh(
                    side_plate(&inner, 0.02, 0.006, x * 1.24),
                    p.graphite.clone(),
                ),
            );
        }
        scene.add_to(
            j1,
            Node::mesh(
                Geometry::rounded_box(0.42, 0.18, 0.44, 0.05, 3).translate(0.0, 0.13, 0.02),
                p.white.clone(),
            ),
        );
        let vents = Geometry::merge((0..6).map(|i| {
            Geometry::rounded_box(0.22, 0.012, 0.01, 0.004, 1).translate(
                0.0,
                0.08 + i as f32 * 0.022,
                0.245,
            )
        }));
        scene.add_to(j1, Node::mesh(vents, p.dark.clone()));
        motor(scene, j1, p, vec3(0.0, 0.1, -0.33), Vec3::Y, 0.1, 0.3);
        motor(
            scene,
            j1,
            p,
            vec3(-0.33, J2_UP, A1),
            Vec3::NEG_X,
            0.125,
            0.22,
        );
        scene.add_to(
            j1,
            Node::mesh(axle(0.2, 0.42, 0.0, J2_UP, A1), p.graphite.clone()),
        );
        for x in [-0.302f32, 0.302] {
            scene.add_to(
                j1,
                Node::mesh(axle(0.12, 0.02, x, J2_UP, A1), p.chrome.clone()),
            );
            scene.add_to(
                j1,
                Node::mesh(
                    bolt_circle(
                        10,
                        0.155,
                        0.01,
                        vec3(x * 1.03, J2_UP, A1),
                        Vec3::X * x.signum(),
                    ),
                    p.chrome.clone(),
                ),
            );
        }
        warning(scene, j1, p, vec3(0.30, 0.17, -0.02), Vec3::X, 0.045);
        lettering(
            scene,
            j1,
            &p.dark,
            font,
            "07",
            0.11,
            vec3(-0.30, 0.18, -0.03),
            Quat::from_rotation_y(-FRAC_PI_2),
        );

        // ---- J2: the upper arm.
        let j2 = scene.add_to(j1, Node::group());
        let beam = hull(&[(vec2(0.0, 0.0), 0.19), (vec2(0.0, L2), 0.15)]);
        scene.add_to(
            j2,
            Node::mesh(side_plate(&beam, 0.3, 0.035, 0.0), p.white.clone()),
        );
        let panel = hull(&[(vec2(0.0, 0.25), 0.1), (vec2(0.0, L2 - 0.25), 0.08)]);
        for x in [-0.155f32, 0.155] {
            scene.add_to(
                j2,
                Node::mesh(side_plate(&panel, 0.016, 0.005, x), p.graphite.clone()),
            );
            let side = if x > 0.0 { FRAC_PI_2 } else { -FRAC_PI_2 };
            lettering(
                scene,
                j2,
                &p.chrome,
                font,
                "FFRAMES",
                0.06,
                vec3(x * 1.06, L2 * 0.52, 0.0),
                Quat::from_rotation_y(side) * Quat::from_rotation_z(FRAC_PI_2),
            );
        }
        scene.add_to(
            j2,
            Node::mesh(
                Geometry::rounded_box(0.004, L2 * 0.5, 0.012, 0.002, 1).translate(
                    0.165,
                    L2 * 0.5,
                    0.11,
                ),
                Material::standard(0x111111).emissive(0xff4a00, 1.6),
            ),
        );
        let cable: Vec<Vec3> = (0..=16)
            .map(|i| {
                let t = i as f32 / 16.0;
                vec3(0.09, 0.08 + t * (L2 - 0.16), -0.2 - 0.05 * (t * PI).sin())
            })
            .collect();
        let (hose, rings) = conduit(&cable, 0.024);
        scene.add_to(j2, Node::mesh(hose, p.rubber.clone()));
        scene.add_to(j2, Node::mesh(rings, p.rubber.clone()));
        let cable2: Vec<Vec3> = cable.iter().map(|c| *c + vec3(-0.07, 0.0, 0.005)).collect();
        scene.add_to(
            j2,
            Node::mesh(Geometry::tube(&cable2, 0.014, 12), p.dark.clone()),
        );
        let clamps = Geometry::merge((1..4).map(|i| {
            Geometry::rounded_box(0.16, 0.03, 0.07, 0.01, 2).translate(
                0.055,
                0.08 + i as f32 * 0.25,
                -0.2 - 0.05 * (i as f32 * 0.25 / 0.9 * PI).sin(),
            )
        }));
        scene.add_to(j2, Node::mesh(clamps, p.chrome.clone()));
        for y in [0.35f32, 0.75] {
            scene.add_to(
                j2,
                Node::mesh(
                    Geometry::torus(0.022, 0.006, 8, 20).translate(0.0, y, 0.17),
                    p.chrome.clone(),
                ),
            );
        }
        scene.add_to(
            j2,
            Node::mesh(axle(0.16, 0.36, 0.0, L2, 0.0), p.graphite.clone()),
        );
        for x in [-0.19f32, 0.19] {
            scene.add_to(
                j2,
                Node::mesh(axle(0.09, 0.02, x, L2, 0.0), p.chrome.clone()),
            );
            scene.add_to(
                j2,
                Node::mesh(
                    bolt_circle(
                        8,
                        0.12,
                        0.009,
                        vec3(x * 1.06, L2, 0.0),
                        Vec3::X * x.signum(),
                    ),
                    p.chrome.clone(),
                ),
            );
        }
        motor(scene, j2, p, vec3(0.18, L2, 0.0), Vec3::X, 0.1, 0.16);

        // ---- J3: the forearm, with the three wrist motors at the back.
        let j3 = scene.add_to(j2, Node::group());
        scene.add_to(
            j3,
            Node::mesh(
                Geometry::rounded_box(0.3, 0.3, 0.34, 0.07, 4).translate(0.0, A3, -0.18),
                p.white.clone(),
            ),
        );
        let grille = Geometry::merge((0..5).map(|i| {
            Geometry::rounded_box(0.012, 0.16, 0.01, 0.004, 1).translate(
                0.152,
                A3,
                -0.26 + i as f32 * 0.03,
            )
        }));
        scene.add_to(j3, Node::mesh(grille.clone(), p.dark.clone()));
        scene.add_to(j3, Node::mesh(grille.scale(-1.0, 1.0, 1.0), p.dark.clone()));
        for (x, y) in [(-0.07f32, 0.07f32), (0.07, 0.07), (0.0, -0.07)] {
            motor(
                scene,
                j3,
                p,
                vec3(x, A3 + y, -0.35),
                Vec3::NEG_Z,
                0.05,
                0.12,
            );
        }
        scene.add_to(
            j3,
            Node::mesh(
                Geometry::cylinder(0.105, 0.13, 0.5, 64)
                    .rotate_x(deg(90.0))
                    .translate(0.0, A3, 0.27),
                p.white.clone(),
            ),
        );
        scene.add_to(
            j3,
            Node::mesh(
                Geometry::torus(0.11, 0.012, 10, 64)
                    .translate(0.0, A3, 0.535)
                    .rotate_x(0.0),
                p.graphite.clone(),
            ),
        );
        let leds = Geometry::merge((0..3).map(|i| {
            Geometry::sphere(0.007, 12, 6).translate(0.0, A3 + 0.12, 0.2 + i as f32 * 0.03)
        }));
        scene.add_to(
            j3,
            Node::mesh(leds, Material::standard(0x111111).emissive(0x7dffb0, 3.0)),
        );
        warning(scene, j3, p, vec3(0.118, A3 + 0.02, 0.35), Vec3::X, 0.035);

        // ---- J4: the front of the forearm rolls.
        let j4 = scene.add_to(j3, Node::group());
        scene.add_to(
            j4,
            Node::mesh(
                Geometry::torus(0.1, 0.01, 10, 64)
                    .translate(0.0, 0.0, -0.06)
                    .rotate_x(0.0),
                p.graphite.clone(),
            ),
        );
        scene.add_to(
            j4,
            Node::mesh(
                Geometry::cylinder(0.095, 0.1, 0.24, 64)
                    .rotate_x(deg(90.0))
                    .translate(0.0, 0.0, 0.06),
                p.white.clone(),
            ),
        );
        scene.add_to(
            j4,
            Node::mesh(
                Geometry::torus(0.092, 0.008, 8, 64)
                    .rotate_x(deg(90.0))
                    .translate(0.0, 0.0, 0.19),
                p.graphite.clone(),
            ),
        );
        scene.add_to(
            j4,
            Node::mesh(
                bolt_circle(8, 0.075, 0.006, vec3(0.0, 0.0, 0.18), Vec3::Z),
                p.chrome.clone(),
            ),
        );
        let fork = hull(&[(vec2(0.2, 0.0), 0.06), (vec2(L3 - J4_AT, 0.0), 0.075)]);
        for x in [-0.085f32, 0.085] {
            scene.add_to(
                j4,
                Node::mesh(side_plate(&fork, 0.03, 0.008, x), p.graphite.clone()),
            );
        }

        // ---- J5: the wrist.
        let j5 = scene.add_to(j4, Node::group());
        scene.add_to(
            j5,
            Node::mesh(axle(0.065, 0.14, 0.0, 0.0, 0.0), p.white.clone()),
        );
        for x in [-0.104f32, 0.104] {
            scene.add_to(
                j5,
                Node::mesh(axle(0.045, 0.012, x, 0.0, 0.0), p.chrome.clone()),
            );
            scene.add_to(
                j5,
                Node::mesh(
                    bolt_circle(
                        5,
                        0.032,
                        0.005,
                        vec3(x * 1.08, 0.0, 0.0),
                        Vec3::X * x.signum(),
                    ),
                    p.dark.clone(),
                ),
            );
        }
        scene.add_to(
            j5,
            Node::mesh(
                Geometry::cylinder(0.055, 0.06, 0.13, 48)
                    .rotate_x(deg(90.0))
                    .translate(0.0, 0.0, 0.08),
                p.white.clone(),
            ),
        );

        // ---- J6: the flange, the tool changer, the gripper and the eye.
        let j6 = scene.add_to(j5, Node::group());
        scene.add_to(
            j6,
            Node::mesh(
                Geometry::cylinder(0.068, 0.068, 0.022, 48)
                    .rotate_x(deg(90.0))
                    .translate(0.0, 0.0, 0.011),
                p.chrome.clone(),
            ),
        );
        scene.add_to(
            j6,
            Node::mesh(
                bolt_circle(6, 0.05, 0.007, vec3(0.0, 0.0, 0.022), Vec3::Z),
                p.dark.clone(),
            ),
        );
        scene.add_to(
            j6,
            Node::mesh(
                Geometry::cylinder(0.058, 0.058, 0.03, 48)
                    .rotate_x(deg(90.0))
                    .translate(0.0, 0.0, 0.04),
                p.graphite.clone(),
            ),
        );
        scene.add_to(
            j6,
            Node::mesh(
                Geometry::torus(0.058, 0.004, 6, 48)
                    .rotate_x(deg(90.0))
                    .translate(0.0, 0.0, 0.056),
                p.chrome.clone(),
            ),
        );
        scene.add_to(
            j6,
            Node::mesh(
                Geometry::rounded_box(0.26, 0.085, 0.08, 0.02, 3).translate(0.0, 0.0, 0.104),
                p.graphite.clone(),
            ),
        );
        for y in [0.035f32, -0.035] {
            scene.add_to(
                j6,
                Node::mesh(
                    Geometry::rounded_box(0.24, 0.016, 0.014, 0.005, 1).translate(0.0, y, 0.15),
                    p.chrome.clone(),
                ),
            );
        }
        let screws = Geometry::merge(
            [-0.1f32, -0.05, 0.05, 0.1]
                .iter()
                .map(|x| rod(0.006, 0.006, vec3(*x, -0.044, 0.104), Vec3::NEG_Y, 6)),
        );
        scene.add_to(j6, Node::mesh(screws, p.chrome.clone()));
        for x in [-0.07f32, 0.07] {
            scene.add_to(
                j6,
                Node::mesh(
                    rod(0.011, 0.03, vec3(x, -0.05, 0.08), Vec3::NEG_Y, 12),
                    p.chrome.clone(),
                ),
            );
            let hose = Geometry::tube(
                &[
                    vec3(x, -0.06, 0.08),
                    vec3(x * 0.8, -0.1, 0.03),
                    vec3(x * 0.4, -0.1, -0.05),
                    vec3(0.0, -0.08, -0.12),
                ],
                0.007,
                8,
            );
            scene.add_to(j6, Node::mesh(hose, p.rubber.clone()));
        }
        let status = scene.add_to(
            j6,
            Node::mesh(
                Geometry::sphere(0.008, 16, 8).translate(0.11, -0.043, 0.09),
                Material::standard(0x111111).emissive(0xff4a00, 0.0),
            ),
        );
        // The eye is a camera module on top of the gripper. It looks along the tool axis.
        let cam = Geometry::rounded_box(0.11, 0.06, 0.09, 0.018, 3).translate(
            EYE.x,
            EYE.y - 0.002,
            EYE.z - 0.04,
        );
        scene.add_to(j6, Node::mesh(cam, p.white.clone()));
        scene.add_to(
            j6,
            Node::mesh(
                Geometry::cylinder(0.026, 0.03, 0.03, 40)
                    .rotate_x(deg(90.0))
                    .translate(EYE.x, EYE.y, EYE.z + 0.015),
                p.graphite.clone(),
            ),
        );
        scene.add_to(
            j6,
            Node::mesh(
                Geometry::torus(0.024, 0.004, 8, 40).translate(EYE.x, EYE.y, EYE.z + 0.03),
                p.chrome.clone(),
            ),
        );
        scene.add_to(
            j6,
            Node::mesh(
                Geometry::sphere_partial(0.021, 32, 12, 0.0, deg(60.0))
                    .rotate_x(deg(90.0))
                    .translate(EYE.x, EYE.y, EYE.z + 0.02),
                p.glass.clone(),
            ),
        );
        let iris = scene.add_to(
            j6,
            Node::mesh(
                Geometry::circle(0.008, 24),
                Material::standard(0x111111).emissive(0xffb070, 0.0),
            )
            .at(EYE.x, EYE.y, EYE.z + 0.0325),
        );
        let lid = scene.add_to(
            j6,
            Node::mesh(
                Geometry::rounded_box(0.056, 0.03, 0.006, 0.003, 1),
                p.graphite.clone(),
            )
            .at(EYE.x, EYE.y + 0.03, EYE.z + 0.036),
        );
        // The eye lights what it looks at, along the tool axis (+Z). In the dark lab its
        // beam shows in the air: a glow cone, see `effects::beam`.
        let lens = vec3(EYE.x, EYE.y, EYE.z + 0.034);
        let eye_light = scene.add_to(
            j6,
            Node::light(Light::spot(EYE_LIGHT, 0.0, deg(17.0), 0.55).pointing(Vec3::Z))
                .with_position(lens),
        );
        let beam = scene.add_to(
            j6,
            Node::mesh(
                crate::effects::beam(2.6, 0.8),
                Material::glow(EYE_LIGHT, 0.0),
            )
            .with_position(lens)
            .shadows(false, false),
        );
        let fingers = [-1.0f32, 1.0].map(|x| {
            let finger = scene.add_to(j6, Node::group());
            scene.add_to(
                finger,
                Node::mesh(
                    Geometry::rounded_box(0.03, 0.06, 0.07, 0.008, 2).translate(
                        x * 0.015,
                        0.0,
                        0.155,
                    ),
                    p.graphite.clone(),
                ),
            );
            scene.add_to(
                finger,
                Node::mesh(
                    Geometry::rounded_box(0.024, 0.055, 0.12, 0.007, 2).translate(
                        x * 0.012,
                        0.0,
                        0.215,
                    ),
                    p.white.clone(),
                ),
            );
            scene.add_to(
                finger,
                Node::mesh(
                    Geometry::rounded_box(0.008, 0.05, 0.06, 0.003, 1).translate(
                        -x * 0.003,
                        0.0,
                        0.245,
                    ),
                    p.rubber.clone(),
                ),
            );
            let bolts = Geometry::merge(
                [0.14f32, 0.17]
                    .iter()
                    .map(|z| rod(0.005, 0.004, vec3(x * 0.031, 0.0, *z), Vec3::X * x, 6)),
            );
            scene.add_to(finger, Node::mesh(bolts, p.chrome.clone()));
            finger
        });

        // A gas spring between the turret and the upper arm balances the arm.
        let spring_body = scene.add(Node::group());
        scene.add_to(
            spring_body,
            Node::mesh(
                Geometry::cylinder(0.045, 0.045, 0.42, 32).translate(0.0, 0.21, 0.0),
                p.graphite.clone(),
            ),
        );
        scene.add_to(
            spring_body,
            Node::mesh(
                Geometry::torus(0.046, 0.005, 6, 32).translate(0.0, 0.4, 0.0),
                p.chrome.clone(),
            ),
        );
        scene.add_to(
            spring_body,
            Node::mesh(Geometry::sphere(0.05, 24, 12), p.chrome.clone()),
        );
        let spring_rod = scene.add(Node::group());
        scene.add_to(
            spring_rod,
            Node::mesh(
                Geometry::cylinder(0.018, 0.018, 0.45, 16).translate(0.0, 0.225, 0.0),
                p.chrome.clone(),
            ),
        );
        scene.add_to(
            spring_rod,
            Node::mesh(Geometry::sphere(0.035, 24, 12), p.chrome.clone()),
        );

        Arm {
            joints: [j1, j2, j3, j4, j5, j6],
            fingers,
            spring_body,
            spring_rod,
            status,
            ring,
            iris,
            lid,
            eye_light,
            beam,
            ring_light,
        }
    }

    /// Poses the arm. `gap` is half the distance between the finger pads.
    pub fn apply(&self, scene: &mut Scene, q: &Joints, gap: f32, face: &Expression) {
        for (node, local) in self.joints.iter().zip(chain(q)) {
            let (_, rotation, translation) = local.to_scale_rotation_translation();
            let n = &mut scene[*node];
            n.position = translation;
            n.rotation = rotation;
        }
        let gap = gap.clamp(0.0, OPEN);
        scene[self.fingers[0]].position = vec3(-gap, 0.0, 0.0);
        scene[self.fingers[1]].position = vec3(gap, 0.0, 0.0);

        let a = scene.local_to_world(self.joints[0], vec3(0.0, 0.16, -0.27));
        let b = scene.local_to_world(self.joints[1], vec3(0.0, 0.42, -0.19));
        let dir = (b - a).normalize_or(Vec3::Y);
        scene[self.spring_body].position = a;
        scene[self.spring_body].rotation = Quat::from_rotation_arc(Vec3::Y, dir);
        scene[self.spring_rod].position = b;
        scene[self.spring_rod].rotation = Quat::from_rotation_arc(Vec3::Y, -dir);

        let glow = |scene: &mut Scene, node: NodeId, value: f32| {
            if let Some(m) = scene[node].material_mut() {
                m.emissive_intensity = value;
            }
        };
        glow(scene, self.ring, face.ring);
        glow(scene, self.status, face.status);
        glow(scene, self.iris, face.eye);
        let lid = face.lid.clamp(0.0, 1.0);
        scene[self.iris].scale = vec3(1.0, lid.max(0.05), 1.0);
        scene[self.lid].position = vec3(EYE.x, EYE.y + 0.012 + 0.018 * lid, EYE.z + 0.036);
        scene[self.lid].scale = vec3(1.0, 1.0 - 0.7 * lid, 1.0);

        // The lights follow the glow: the eye light is off while the lid is closed.
        let eye = face.eye * lid;
        let set_light = |scene: &mut Scene, node: NodeId, value: f32| {
            if let Some(light) = scene[node].light_mut() {
                *light.intensity_mut() = value;
            }
        };
        set_light(scene, self.eye_light, eye * 0.55);
        set_light(scene, self.ring_light, face.ring * 0.08);
        let beam = eye * face.beam;
        scene[self.beam].visible = beam > 0.0;
        if let Some(m) = scene[self.beam].material_mut() {
            m.color = Rgb::hex(EYE_LIGHT) * beam * 0.05;
        }
    }
}
