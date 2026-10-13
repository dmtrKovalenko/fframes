//! The robot: a skeleton of dark mechanical parts and the car body panels that become its
//! armor. During the transformation each panel flies from the car to its place on a bone.

use fframes_3d::math::smoothstep;
use fframes_3d::*;

/// The scale of the robot. The model has near human proportions, and the scale makes it 3.7 m
/// tall.
pub const SCALE: f32 = 1.3;

/// The pelvis above the feet in the rest pose, before [`SCALE`].
pub const PELVIS: Vec3 = Vec3::new(0.0, 1.32, 0.0);

/// The bones of the skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bone {
    Pelvis,
    Spine,
    Chest,
    Neck,
    ShoulderL,
    ElbowL,
    WristL,
    ShoulderR,
    ElbowR,
    WristR,
    HipL,
    KneeL,
    AnkleL,
    HipR,
    KneeR,
    AnkleR,
}

/// The number of bones.
pub const BONES: usize = 16;

impl Bone {
    pub const ALL: [Bone; BONES] = [
        Bone::Pelvis,
        Bone::Spine,
        Bone::Chest,
        Bone::Neck,
        Bone::ShoulderL,
        Bone::ElbowL,
        Bone::WristL,
        Bone::ShoulderR,
        Bone::ElbowR,
        Bone::WristR,
        Bone::HipL,
        Bone::KneeL,
        Bone::AnkleL,
        Bone::HipR,
        Bone::KneeR,
        Bone::AnkleR,
    ];

    /// The parent bone and the offset from it in the rest pose. The robot faces +Z and its
    /// left is +X.
    fn layout(self) -> (Option<Bone>, Vec3) {
        match self {
            Bone::Pelvis => (None, PELVIS),
            Bone::Spine => (Some(Bone::Pelvis), vec3(0.0, 0.2, 0.0)),
            Bone::Chest => (Some(Bone::Spine), vec3(0.0, 0.36, 0.0)),
            Bone::Neck => (Some(Bone::Chest), vec3(0.0, 0.42, 0.02)),
            Bone::ShoulderL => (Some(Bone::Chest), vec3(0.64, 0.22, 0.0)),
            Bone::ElbowL => (Some(Bone::ShoulderL), vec3(0.0, -0.56, 0.0)),
            Bone::WristL => (Some(Bone::ElbowL), vec3(0.0, -0.5, 0.0)),
            Bone::ShoulderR => (Some(Bone::Chest), vec3(-0.64, 0.22, 0.0)),
            Bone::ElbowR => (Some(Bone::ShoulderR), vec3(0.0, -0.56, 0.0)),
            Bone::WristR => (Some(Bone::ElbowR), vec3(0.0, -0.5, 0.0)),
            Bone::HipL => (Some(Bone::Pelvis), vec3(0.25, -0.1, 0.0)),
            Bone::KneeL => (Some(Bone::HipL), vec3(0.0, -0.58, 0.0)),
            Bone::AnkleL => (Some(Bone::KneeL), vec3(0.0, -0.5, 0.0)),
            Bone::HipR => (Some(Bone::Pelvis), vec3(-0.25, -0.1, 0.0)),
            Bone::KneeR => (Some(Bone::HipR), vec3(0.0, -0.58, 0.0)),
            Bone::AnkleR => (Some(Bone::KneeR), vec3(0.0, -0.5, 0.0)),
        }
    }
}

/// The joint rotations relative to the rest pose, and an offset of the pelvis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub joints: [Quat; BONES],
    pub pelvis: Vec3,
}

impl Default for Pose {
    fn default() -> Self {
        Self {
            joints: [Quat::IDENTITY; BONES],
            pelvis: Vec3::ZERO,
        }
    }
}

impl Pose {
    /// Sets a joint from Euler angles in degrees.
    pub fn with(mut self, bone: Bone, x: f32, y: f32, z: f32) -> Self {
        self.joints[bone as usize] = euler_deg(x, y, z);
        self
    }

    /// Adds a rotation of `angle` radians about X to the joint of `bone`.
    pub fn with_extra(mut self, bone: Bone, angle: f32) -> Self {
        self.joints[bone as usize] *= Quat::from_rotation_x(angle);
        self
    }

    /// Sets the pelvis offset.
    pub fn pelvis(mut self, offset: Vec3) -> Self {
        self.pelvis = offset;
        self
    }

    /// Interpolates every joint and the pelvis offset.
    pub fn lerp(&self, other: &Pose, t: f32) -> Pose {
        Pose {
            joints: std::array::from_fn(|i| self.joints[i].slerp(other.joints[i], t)),
            pelvis: self.pelvis.lerp(other.pelvis, t),
        }
    }

    /// Straight, as the skeleton lies in the car.
    pub fn packed() -> Pose {
        Pose::default()
            .with(Bone::Neck, 35.0, 0.0, 0.0)
            .with(Bone::AnkleL, 60.0, 0.0, 0.0)
            .with(Bone::AnkleR, 60.0, 0.0, 0.0)
            .with(Bone::ShoulderL, 0.0, 0.0, -6.0)
            .with(Bone::ShoulderR, 0.0, 0.0, 6.0)
    }

    /// The skeleton curls into a ball during the flip.
    pub fn tuck() -> Pose {
        Pose::default()
            .with(Bone::Spine, 25.0, 0.0, 0.0)
            .with(Bone::Chest, 15.0, 0.0, 0.0)
            .with(Bone::Neck, 20.0, 0.0, 0.0)
            .with(Bone::ShoulderL, -50.0, 0.0, 20.0)
            .with(Bone::ShoulderR, -50.0, 0.0, -20.0)
            .with(Bone::ElbowL, -100.0, 0.0, 0.0)
            .with(Bone::ElbowR, -100.0, 0.0, 0.0)
            .with(Bone::HipL, -105.0, 0.0, 8.0)
            .with(Bone::HipR, -105.0, 0.0, -8.0)
            .with(Bone::KneeL, 125.0, 0.0, 0.0)
            .with(Bone::KneeR, 125.0, 0.0, 0.0)
    }

    /// The hero landing: right knee and right fist on the ground, left foot forward.
    pub fn landing() -> Pose {
        Pose::default()
            .pelvis(vec3(0.0, -0.62, -0.05))
            .with(Bone::Pelvis, 10.0, 8.0, 0.0)
            .with(Bone::Spine, 22.0, 0.0, 0.0)
            .with(Bone::Chest, 14.0, -8.0, 0.0)
            .with(Bone::Neck, -32.0, 6.0, 0.0)
            .with(Bone::HipL, -88.0, 0.0, 14.0)
            .with(Bone::KneeL, 92.0, 0.0, 0.0)
            .with(Bone::AnkleL, -12.0, 0.0, 0.0)
            .with(Bone::HipR, -8.0, 0.0, -10.0)
            .with(Bone::KneeR, 110.0, 0.0, 0.0)
            .with(Bone::AnkleR, 55.0, 0.0, 0.0)
            .with(Bone::ShoulderR, -42.0, 0.0, -6.0)
            .with(Bone::ElbowR, -6.0, 0.0, 0.0)
            .with(Bone::ShoulderL, 35.0, 0.0, 38.0)
            .with(Bone::ElbowL, -55.0, 0.0, 0.0)
    }

    /// The robot stands tall with the fists low and wide.
    pub fn stand() -> Pose {
        Pose::default()
            .with(Bone::Chest, -4.0, 0.0, 0.0)
            .with(Bone::Neck, 2.0, 0.0, 0.0)
            .with(Bone::ShoulderL, 4.0, 0.0, 16.0)
            .with(Bone::ShoulderR, 4.0, 0.0, -16.0)
            .with(Bone::ElbowL, -22.0, 0.0, 0.0)
            .with(Bone::ElbowR, -22.0, 0.0, 0.0)
            .with(Bone::HipL, -4.0, 0.0, 9.0)
            .with(Bone::HipR, 4.0, 0.0, -9.0)
            .with(Bone::KneeL, 6.0, 0.0, 0.0)
            .with(Bone::KneeR, 4.0, 0.0, 0.0)
            .with(Bone::AnkleL, 0.0, 0.0, -9.0)
            .with(Bone::AnkleR, 0.0, 0.0, 9.0)
    }

    /// The right arm aims the cannon at the camera, and the left hand supports it.
    pub fn aim() -> Pose {
        Pose::default()
            .pelvis(vec3(0.0, -0.12, 0.0))
            .with(Bone::Pelvis, 0.0, -18.0, 0.0)
            .with(Bone::Spine, 6.0, 8.0, 0.0)
            .with(Bone::Chest, 2.0, 14.0, 0.0)
            .with(Bone::Neck, 4.0, -6.0, 0.0)
            .with(Bone::ShoulderR, -86.0, -8.0, -4.0)
            .with(Bone::ElbowR, -4.0, 0.0, 0.0)
            .with(Bone::WristR, 0.0, 0.0, 0.0)
            .with(Bone::ShoulderL, -58.0, 0.0, -28.0)
            .with(Bone::ElbowL, -78.0, 0.0, 0.0)
            .with(Bone::HipL, -22.0, 0.0, 12.0)
            .with(Bone::KneeL, 26.0, 0.0, 0.0)
            .with(Bone::AnkleL, 0.0, 0.0, -10.0)
            .with(Bone::HipR, 16.0, 0.0, -12.0)
            .with(Bone::KneeR, 14.0, 0.0, 0.0)
            .with(Bone::AnkleR, -12.0, 0.0, 10.0)
    }

    /// A deep crouch before a jump.
    pub fn crouch() -> Pose {
        Pose::default()
            .pelvis(vec3(0.0, -0.45, -0.1))
            .with(Bone::Spine, 28.0, 0.0, 0.0)
            .with(Bone::Chest, 12.0, 0.0, 0.0)
            .with(Bone::Neck, -30.0, 0.0, 0.0)
            .with(Bone::ShoulderL, 40.0, 0.0, 20.0)
            .with(Bone::ShoulderR, 40.0, 0.0, -20.0)
            .with(Bone::ElbowL, -30.0, 0.0, 0.0)
            .with(Bone::ElbowR, -30.0, 0.0, 0.0)
            .with(Bone::HipL, -75.0, 0.0, 10.0)
            .with(Bone::HipR, -75.0, 0.0, -10.0)
            .with(Bone::KneeL, 100.0, 0.0, 0.0)
            .with(Bone::KneeR, 100.0, 0.0, 0.0)
            .with(Bone::AnkleL, -25.0, 0.0, 0.0)
            .with(Bone::AnkleR, -25.0, 0.0, 0.0)
    }
}

/// The skeleton in car space: face down, with the feet at the rear and the head behind the
/// hood.
pub fn packed_offset() -> Mat4 {
    Mat4::from_translation(vec3(0.0, 0.55, -2.0))
        * Mat4::from_rotation_x(deg(90.0))
        * Mat4::from_scale(Vec3::splat(SCALE))
}

/// A car body panel, with its place on the car and its place on the robot.
#[derive(Debug, Clone)]
pub struct Panel {
    pub node: NodeId,
    pub name: &'static str,
    /// The transform in car space.
    pub car: Mat4,
    pub bone: Bone,
    /// The transform in the space of `bone`, before [`SCALE`].
    pub robot: Mat4,
    /// The start of the move, as a fraction of the transformation (`0..1`).
    pub delay: f32,
    /// An extra spin at the middle of the move: the axis times the angle in radians, in
    /// world space.
    pub spin: Vec3,
    /// How far the path curves away from the body, in meters.
    pub arc: f32,
    /// The panel is a wheel and turns with the car.
    pub wheel: bool,
}

/// The materials of the robot and the car.
pub struct Palette {
    pub paint: Material,
    pub stripe: Material,
    pub frame: Material,
    pub frame_light: Material,
    pub chrome: Material,
    pub trim: Material,
    pub glass: Material,
    pub rubber: Material,
    pub glow: Material,
    pub headlight: Material,
    pub taillight: Material,
    pub caliper: Material,
}

impl Palette {
    pub fn new() -> Self {
        Self {
            paint: Material::standard(0xf2380c)
                .metalness(0.25)
                .roughness(0.34)
                .clearcoat(1.0),
            stripe: Material::standard(0x0b0c0e).roughness(0.3).clearcoat(1.0),
            frame: Material::metal(0x30343c, 0.42),
            frame_light: Material::metal(0x5d636d, 0.32),
            chrome: Material::metal(0xe4e8ee, 0.1),
            trim: Material::standard(0x0d0e11).roughness(0.55),
            glass: Material::standard(0x040507)
                .roughness(0.04)
                .env_intensity(1.6),
            rubber: Material::standard(0x0e0e10).roughness(0.85),
            glow: Material::standard(0x000000).emissive(0x22e8ff, 0.0),
            headlight: Material::standard(0x101418).emissive(0xd8f0ff, 0.0),
            taillight: Material::standard(0x140406).emissive(0xff1430, 0.0),
            caliper: Material::standard(0xffc21a).roughness(0.4),
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::new()
    }
}

/// The kind of a glowing part. [`RobotState`] sets the brightness of each kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glow {
    Eyes,
    Reactor,
    Seams,
    Headlights,
    Taillights,
    Cannon,
}

/// The animated state of the robot at one moment.
#[derive(Debug, Clone, Copy)]
pub struct RobotState {
    /// World transform of the car body.
    pub car: Mat4,
    /// World transform of the skeleton root (the feet), with [`SCALE`].
    pub root: Mat4,
    pub pose: Pose,
    /// 0 is the car, 1 is the robot.
    pub morph: f32,
    /// Wheel rotation in radians.
    pub wheel_spin: f32,
    pub eyes: f32,
    pub reactor: f32,
    pub seams: f32,
    pub headlights: f32,
    pub taillights: f32,
    /// The right forearm opens into a cannon, from 0 to 1.
    pub cannon: f32,
    /// Glow of the cannon muzzle.
    pub charge: f32,
    /// The chest plates slide apart over the reactor, from 0 to 1.
    pub chest_open: f32,
}

/// The scene nodes of the robot.
pub struct Robot {
    pub root: NodeId,
    pub bones: [NodeId; BONES],
    pub panels: Vec<Panel>,
    glows: Vec<(NodeId, Glow, f32)>,
    cannon_barrel: NodeId,
    cannon_shroud: NodeId,
    hand_r: NodeId,
    pub muzzle: NodeId,
    rest: [Vec3; BONES],
    /// Points on the soles, the knees and the right fist that can touch the ground.
    contacts: Vec<(Bone, Vec3)>,
}

/// A mesh node with a copy of `material`.
fn mesh(geometry: Geometry, material: &Material) -> Node {
    Node::mesh(geometry, material.clone())
}

impl Robot {
    /// Adds the robot (skeleton and car panels) to `scene`.
    pub fn build(scene: &mut Scene, palette: &Palette) -> Robot {
        let p = palette;
        let root = scene.add(Node::group().named("robot"));
        let mut bones = [root; BONES];
        let mut rest = [Vec3::ZERO; BONES];
        for bone in Bone::ALL {
            let (parent, offset) = bone.layout();
            let parent = parent.map_or(root, |b| bones[b as usize]);
            bones[bone as usize] = scene.add_to(parent, Node::group().with_position(offset));
            rest[bone as usize] = offset;
        }
        let b = |bone: Bone| bones[bone as usize];
        let mut glows = Vec::new();

        // Pelvis.
        scene.add_to(
            b(Bone::Pelvis),
            mesh(Geometry::rounded_box(0.6, 0.28, 0.4, 0.06, 3), &p.frame).at(0.0, -0.03, 0.0),
        );
        scene.add_to(
            b(Bone::Pelvis),
            mesh(
                Geometry::rounded_box(0.3, 0.18, 0.46, 0.04, 2),
                &p.frame_light,
            )
            .at(0.0, -0.12, 0.0),
        );
        for side in [-1.0, 1.0] {
            scene.add_to(
                b(Bone::Pelvis),
                mesh(Geometry::sphere(0.13, 24, 12), &p.frame_light).at(side * 0.25, -0.1, 0.0),
            );
        }

        // Spine: a stack of vertebrae and two pistons.
        scene.add_to(
            b(Bone::Spine),
            mesh(Geometry::cylinder(0.17, 0.2, 0.36, 24), &p.frame).at(0.0, 0.1, 0.0),
        );
        for i in 0..3 {
            scene.add_to(
                b(Bone::Spine),
                mesh(
                    Geometry::torus(0.19, 0.035, 10, 32).rotate_x(deg(90.0)),
                    &p.frame_light,
                )
                .at(0.0, i as f32 * 0.11, 0.0),
            );
        }
        for side in [-1.0, 1.0] {
            scene.add_to(
                b(Bone::Spine),
                mesh(Geometry::cylinder(0.025, 0.025, 0.42, 12), &p.chrome)
                    .at(side * 0.2, 0.12, -0.05)
                    .rotate_z(side * 0.12),
            );
            let seam = scene.add_to(
                b(Bone::Spine),
                mesh(Geometry::rounded_box(0.03, 0.26, 0.03, 0.01, 1), &p.glow).at(
                    side * 0.12,
                    0.12,
                    0.17,
                ),
            );
            glows.push((seam, Glow::Seams, 1.0));
        }

        // The chest core with the reactor.
        scene.add_to(
            b(Bone::Chest),
            mesh(Geometry::rounded_box(0.92, 0.6, 0.5, 0.09, 3), &p.frame).at(0.0, 0.05, -0.02),
        );
        scene.add_to(
            b(Bone::Chest),
            mesh(
                Geometry::rounded_box(1.0, 0.16, 0.46, 0.05, 2),
                &p.frame_light,
            )
            .at(0.0, 0.32, -0.02),
        );
        scene.add_to(
            b(Bone::Chest),
            mesh(Geometry::torus(0.13, 0.035, 12, 40), &p.chrome).at(0.0, 0.08, 0.235),
        );
        let reactor = scene.add_to(
            b(Bone::Chest),
            mesh(Geometry::circle(0.11, 40), &p.glow).at(0.0, 0.08, 0.232),
        );
        glows.push((reactor, Glow::Reactor, 1.0));
        for side in [-1.0, 1.0] {
            scene.add_to(
                b(Bone::Chest),
                mesh(
                    Geometry::cylinder(0.16, 0.16, 0.2, 24).rotate_z(deg(90.0)),
                    &p.frame_light,
                )
                .at(side * 0.5, 0.22, 0.0),
            );
            let vent = scene.add_to(
                b(Bone::Chest),
                mesh(Geometry::rounded_box(0.22, 0.03, 0.02, 0.01, 1), &p.glow).at(
                    side * 0.28,
                    -0.16,
                    0.235,
                ),
            );
            glows.push((vent, Glow::Seams, 1.0));
        }

        // Neck and head.
        scene.add_to(
            b(Bone::Neck),
            mesh(Geometry::cylinder(0.09, 0.11, 0.2, 16), &p.frame_light).at(0.0, 0.05, 0.0),
        );
        let head = scene.add_to(b(Bone::Neck), Node::group().at(0.0, 0.18, 0.0));
        scene.add_to(
            head,
            mesh(Geometry::rounded_box(0.34, 0.3, 0.36, 0.09, 3), &p.frame).at(0.0, 0.1, 0.0),
        );
        scene.add_to(
            head,
            mesh(Geometry::rounded_box(0.36, 0.12, 0.28, 0.05, 2), &p.paint).at(0.0, 0.24, -0.03),
        );
        scene.add_to(
            head,
            mesh(Geometry::rounded_box(0.3, 0.14, 0.06, 0.03, 2), &p.trim).at(0.0, 0.11, 0.17),
        );
        let visor = scene.add_to(
            head,
            mesh(Geometry::rounded_box(0.26, 0.035, 0.03, 0.012, 1), &p.glow).at(0.0, 0.13, 0.2),
        );
        glows.push((visor, Glow::Eyes, 1.0));
        let crest = Geometry::extrude(
            &[vec2(-0.16, 0.0), vec2(0.12, 0.0), vec2(0.0, 0.16)],
            0.03,
            0.008,
        )
        .rotate_y(deg(90.0));
        scene.add_to(head, mesh(crest, &p.paint).at(0.0, 0.29, 0.02));
        for side in [-1.0, 1.0] {
            let fin = Geometry::extrude(
                &[vec2(-0.1, 0.0), vec2(0.08, 0.0), vec2(-0.04, 0.22)],
                0.03,
                0.008,
            )
            .rotate_y(deg(90.0));
            scene.add_to(
                head,
                mesh(fin, &p.chrome)
                    .at(side * 0.19, 0.12, -0.04)
                    .rotate_z(-side * 0.35),
            );
        }

        // Arms.
        for (shoulder, elbow, wrist, side) in [
            (Bone::ShoulderL, Bone::ElbowL, Bone::WristL, 1.0f32),
            (Bone::ShoulderR, Bone::ElbowR, Bone::WristR, -1.0),
        ] {
            scene.add_to(
                b(shoulder),
                mesh(Geometry::sphere(0.15, 24, 12), &p.frame_light),
            );
            scene.add_to(
                b(shoulder),
                mesh(Geometry::rounded_box(0.22, 0.42, 0.24, 0.06, 2), &p.frame)
                    .at(0.0, -0.28, 0.0),
            );
            scene.add_to(
                b(shoulder),
                mesh(Geometry::cylinder(0.022, 0.022, 0.38, 10), &p.chrome).at(
                    side * 0.03,
                    -0.3,
                    0.15,
                ),
            );
            scene.add_to(
                b(elbow),
                mesh(Geometry::sphere(0.12, 20, 10), &p.frame_light),
            );
            scene.add_to(
                b(elbow),
                mesh(Geometry::rounded_box(0.25, 0.42, 0.27, 0.07, 2), &p.frame)
                    .at(0.0, -0.25, 0.0),
            );
            let seam = scene.add_to(
                b(elbow),
                mesh(Geometry::rounded_box(0.02, 0.24, 0.02, 0.008, 1), &p.glow).at(
                    side * 0.06,
                    -0.27,
                    0.14,
                ),
            );
            glows.push((seam, Glow::Seams, 1.0));
            // Fist.
            let hand = scene.add_to(b(wrist), Node::group());
            scene.add_to(
                hand,
                mesh(Geometry::rounded_box(0.2, 0.17, 0.15, 0.04, 2), &p.frame).at(0.0, -0.08, 0.0),
            );
            for i in 0..4 {
                let x = (i as f32 - 1.5) * 0.05;
                scene.add_to(
                    hand,
                    mesh(
                        Geometry::rounded_box(0.044, 0.08, 0.12, 0.018, 2),
                        &p.frame_light,
                    )
                    .at(x, -0.2, 0.03),
                );
            }
            scene.add_to(
                hand,
                mesh(
                    Geometry::rounded_box(0.05, 0.1, 0.07, 0.02, 2),
                    &p.frame_light,
                )
                .at(-side * 0.12, -0.12, 0.05)
                .rotate_z(side * 0.4),
            );
            if side < 0.0 {
                scene[hand].name = Some("hand_r".into());
            }
        }

        // The right forearm holds a hidden cannon.
        let cannon_shroud = scene.add_to(b(Bone::ElbowR), Node::group().named("cannon"));
        scene.add_to(
            cannon_shroud,
            mesh(Geometry::tube_ring(0.11, 0.075, 0.3, 32), &p.frame_light),
        );
        scene.add_to(
            cannon_shroud,
            mesh(
                Geometry::torus(0.115, 0.018, 8, 32).rotate_x(deg(90.0)),
                &p.chrome,
            )
            .at(0.0, -0.15, 0.0),
        );
        let cannon_barrel = scene.add_to(cannon_shroud, Node::group());
        scene.add_to(
            cannon_barrel,
            mesh(Geometry::tube_ring(0.075, 0.05, 0.42, 32), &p.chrome).at(0.0, -0.2, 0.0),
        );
        let bore = scene.add_to(
            cannon_barrel,
            mesh(Geometry::cylinder(0.05, 0.05, 0.36, 24), &p.glow).at(0.0, -0.2, 0.0),
        );
        glows.push((bore, Glow::Cannon, 1.0));
        let muzzle = scene.add_to(cannon_barrel, Node::group().at(0.0, -0.42, 0.0));

        // Legs.
        for (hip, knee, ankle, side) in [
            (Bone::HipL, Bone::KneeL, Bone::AnkleL, 1.0f32),
            (Bone::HipR, Bone::KneeR, Bone::AnkleR, -1.0),
        ] {
            scene.add_to(
                b(hip),
                mesh(Geometry::rounded_box(0.28, 0.48, 0.3, 0.07, 2), &p.frame).at(0.0, -0.28, 0.0),
            );
            scene.add_to(
                b(hip),
                mesh(Geometry::cylinder(0.026, 0.026, 0.44, 10), &p.chrome).at(
                    side * 0.16,
                    -0.3,
                    0.06,
                ),
            );
            let seam = scene.add_to(
                b(hip),
                mesh(Geometry::rounded_box(0.02, 0.3, 0.02, 0.008, 1), &p.glow).at(
                    -side * 0.06,
                    -0.3,
                    0.155,
                ),
            );
            glows.push((seam, Glow::Seams, 1.0));
            scene.add_to(
                b(knee),
                mesh(Geometry::sphere(0.14, 24, 12), &p.frame_light),
            );
            scene.add_to(
                b(knee),
                mesh(Geometry::rounded_box(0.2, 0.18, 0.12, 0.05, 2), &p.paint)
                    .at(0.0, 0.02, 0.14)
                    .rotate_x(-0.2),
            );
            scene.add_to(
                b(knee),
                mesh(Geometry::rounded_box(0.24, 0.42, 0.26, 0.07, 2), &p.frame)
                    .at(0.0, -0.26, -0.01),
            );
            scene.add_to(
                b(ankle),
                mesh(Geometry::sphere(0.09, 16, 8), &p.frame_light),
            );
            let foot = Geometry::extrude(
                &[
                    vec2(-0.13, -0.12),
                    vec2(0.34, -0.12),
                    vec2(0.34, -0.05),
                    vec2(0.1, 0.03),
                    vec2(-0.1, 0.03),
                ],
                0.24,
                0.03,
            )
            .rotate_y(deg(-90.0));
            scene.add_to(b(ankle), mesh(foot, &p.frame).at(0.0, -0.02, 0.0));
        }

        let contacts = vec![
            (Bone::AnkleL, vec3(0.0, -0.14, 0.34)),
            (Bone::AnkleL, vec3(0.0, -0.14, -0.13)),
            (Bone::AnkleR, vec3(0.0, -0.14, 0.34)),
            (Bone::AnkleR, vec3(0.0, -0.14, -0.13)),
            (Bone::KneeL, vec3(0.0, 0.0, 0.2)),
            (Bone::KneeR, vec3(0.0, 0.0, 0.2)),
            (Bone::WristR, vec3(0.0, -0.25, 0.0)),
        ];

        let panels = build_panels(scene, palette, &mut glows);
        let hand_r = scene.find("hand_r").unwrap_or(root);

        Robot {
            root,
            bones,
            panels,
            glows,
            cannon_barrel,
            cannon_shroud,
            hand_r,
            muzzle,
            rest,
            contacts,
        }
    }

    pub fn bone(&self, bone: Bone) -> NodeId {
        self.bones[bone as usize]
    }

    /// Poses the skeleton but not the panels. [`Robot::ground_offset`] uses it to measure the
    /// contact with the ground.
    fn pose_skeleton(&self, scene: &mut Scene, root: Mat4, pose: &Pose) {
        let (scale, rotation, translation) = root.to_scale_rotation_translation();
        let r = &mut scene[self.root];
        r.position = translation;
        r.rotation = rotation;
        r.scale = scale;
        for bone in Bone::ALL {
            let node = &mut scene[self.bones[bone as usize]];
            node.rotation = pose.joints[bone as usize];
            node.position = self.rest[bone as usize]
                + if bone == Bone::Pelvis {
                    pose.pelvis
                } else {
                    Vec3::ZERO
                };
        }
    }

    /// The depth of the lowest contact point of `pose` below the ground, with the skeleton
    /// root at `root`. Add it to the root height to put the robot on the floor.
    pub fn ground_offset(&self, scene: &mut Scene, root: Mat4, pose: &Pose) -> f32 {
        self.pose_skeleton(scene, root, pose);
        let lowest = self
            .contacts
            .iter()
            .map(|(bone, local)| scene.local_to_world(self.bone(*bone), *local).y)
            .fold(f32::INFINITY, f32::min);
        -lowest
    }

    /// Applies a state: the skeleton pose, the panels between car and robot, and the glows.
    pub fn apply(&self, scene: &mut Scene, s: &RobotState) {
        self.pose_skeleton(scene, s.root, &s.pose);
        // The skeleton is hidden in the car.
        scene[self.root].visible = s.morph > 0.002;

        // The cannon: the hand folds away and the barrel slides out of the forearm.
        let c = s.cannon.clamp(0.0, 1.0);
        scene[self.cannon_shroud].position = vec3(0.0, -0.25 - 0.1 * c, 0.0);
        scene[self.cannon_shroud].scale = Vec3::splat(c.max(0.001));
        scene[self.cannon_shroud].visible = c > 0.0;
        scene[self.cannon_barrel].position = vec3(0.0, -0.22 * smoothstep(0.4, 1.0, c), 0.0);
        scene[self.hand_r].scale = Vec3::splat((1.0 - smoothstep(0.0, 0.5, c)).max(0.001));
        scene[self.hand_r].visible = c < 0.5;

        let world = scene.world_matrices();
        for panel in &self.panels {
            let car = s.car * panel.car;
            let robot = world[self.bone(panel.bone).index()] * panel.robot;
            let t = smoothstep(panel.delay, panel.delay + 0.42, s.morph);
            let (cs, cr, ct) = car.to_scale_rotation_translation();
            let (rs, rr, rt) = robot.to_scale_rotation_translation();
            let ease = ease_back(t);
            let bump = (std::f32::consts::PI * t).sin();
            let away = (ct - s.root.w_axis.truncate()).normalize_or(Vec3::Y);
            let mut rotation = cr.slerp(rr, t);
            if panel.spin != Vec3::ZERO {
                rotation = Quat::from_scaled_axis(panel.spin * bump) * rotation;
            }
            if panel.wheel {
                rotation *= Quat::from_rotation_x(s.wheel_spin * (1.0 - t));
            }
            // The chest plates slide apart to show the reactor.
            let mut position = ct.lerp(rt, ease) + (away + Vec3::Y * 0.5) * (panel.arc * bump);
            if s.chest_open > 0.0 && panel.bone == Bone::Chest && panel.name.starts_with("hood") {
                let side = if panel.name.ends_with('l') { 1.0 } else { -1.0 };
                position += world[self.bone(Bone::Chest).index()].transform_vector3(vec3(
                    side * 0.12,
                    0.0,
                    0.06,
                )) * s.chest_open;
            }
            let node = &mut scene[panel.node];
            node.position = position;
            node.rotation = rotation;
            node.scale = cs.lerp(rs, t);
        }

        for &(node, glow, base) in &self.glows {
            let value = match glow {
                Glow::Eyes => s.eyes,
                Glow::Reactor => s.reactor,
                Glow::Seams => s.seams,
                Glow::Headlights => s.headlights,
                Glow::Taillights => s.taillights,
                Glow::Cannon => s.charge,
            };
            if let Some(material) = scene[node].material_mut() {
                material.emissive_intensity = base * value;
            }
        }
    }
}

/// Goes a little past 1 and comes back, as a part that snaps into place.
fn ease_back(t: f32) -> f32 {
    let c1 = 1.4;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// Turns the parts of a left side assembly around for the right side.
fn mirrored(parts: Vec<Node>, side: f32) -> Vec<Node> {
    if side > 0.0 {
        return parts;
    }
    parts
        .into_iter()
        .map(|mut n| {
            n.position = vec3(-n.position.x, n.position.y, -n.position.z);
            n.rotate_y(deg(180.0))
        })
        .collect()
}

/// A transform from a position, a rotation and a scale.
fn transform(position: Vec3, rotation: Quat, scale: Vec3) -> Mat4 {
    Mat4::from_scale_rotation_translation(scale, rotation, position)
}

/// The car body, cut into the panels that become the armor.
fn build_panels(
    scene: &mut Scene,
    p: &Palette,
    glows: &mut Vec<(NodeId, Glow, f32)>,
) -> Vec<Panel> {
    let mut panels = Vec::new();
    let mut add = |scene: &mut Scene,
                   name: &'static str,
                   parts: Vec<Node>,
                   car: Mat4,
                   bone: Bone,
                   robot: Mat4,
                   delay: f32,
                   spin: Vec3,
                   arc: f32|
     -> NodeId {
        let node = scene.add(Node::group().named(name));
        for part in parts {
            scene.add_to(node, part);
        }
        panels.push(Panel {
            node,
            name,
            car,
            bone,
            robot,
            delay,
            spin,
            arc,
            wheel: name.starts_with("wheel"),
        });
        node
    };

    // Hood halves -> chest plates.
    for (side, name) in [(1.0f32, "hood_l"), (-1.0, "hood_r")] {
        let plate = Geometry::rounded_box(0.84, 0.07, 1.3, 0.035, 3);
        let stripe = Geometry::rounded_box(0.16, 0.075, 1.31, 0.03, 2);
        let parts = vec![
            mesh(plate, &p.paint),
            mesh(stripe, &p.stripe).at(-side * 0.3, 0.004, 0.0),
        ];
        add(
            scene,
            name,
            parts,
            transform(
                vec3(side * 0.43, 0.86, 1.36),
                euler_deg(6.0, 0.0, 0.0),
                Vec3::ONE,
            ),
            Bone::Chest,
            transform(
                vec3(side * 0.25, 0.08, 0.27),
                euler_deg(90.0, 0.0, 0.0) * Quat::from_rotation_y(side * deg(-12.0)),
                vec3(0.56, 1.0, 0.5),
            ),
            0.1,
            vec3(0.0, 0.0, side * 2.0),
            0.6,
        );
    }

    // Front bumper with the headlights -> lower chest.
    {
        let mut parts = vec![
            mesh(Geometry::rounded_box(1.86, 0.5, 0.32, 0.1, 3), &p.paint),
            mesh(Geometry::rounded_box(1.2, 0.16, 0.06, 0.03, 2), &p.trim).at(0.0, -0.12, 0.15),
        ];
        let lights = Node::mesh(
            Geometry::rounded_box(0.42, 0.06, 0.05, 0.02, 2),
            p.headlight.clone(),
        );
        parts.push(lights.clone().at(0.6, 0.12, 0.15));
        parts.push(lights.at(-0.6, 0.12, 0.15));
        let node = add(
            scene,
            "bumper",
            parts,
            transform(vec3(0.0, 0.52, 2.02), Quat::IDENTITY, Vec3::ONE),
            Bone::Chest,
            transform(vec3(0.0, -0.22, 0.25), Quat::IDENTITY, vec3(0.5, 0.42, 0.5)),
            0.18,
            vec3(2.5, 0.0, 0.0),
            0.5,
        );
        for &child in scene[node].children() {
            if scene[child]
                .as_mesh()
                .is_some_and(|m| m.material.emissive == p.headlight.emissive)
            {
                glows.push((child, Glow::Headlights, 1.0));
            }
        }
    }

    // Windshield and roof -> back.
    add(
        scene,
        "windshield",
        vec![mesh(
            Geometry::rounded_box(1.56, 0.04, 0.9, 0.02, 2),
            &p.glass,
        )],
        transform(vec3(0.0, 1.05, 0.29), euler_deg(18.0, 0.0, 0.0), Vec3::ONE),
        Bone::Chest,
        transform(
            vec3(0.0, 0.14, -0.3),
            euler_deg(-100.0, 0.0, 0.0),
            vec3(0.55, 1.0, 0.45),
        ),
        0.3,
        vec3(-3.0, 0.0, 0.0),
        0.8,
    );
    add(
        scene,
        "roof",
        vec![
            mesh(Geometry::rounded_box(1.5, 0.06, 0.74, 0.03, 3), &p.paint),
            mesh(Geometry::rounded_box(0.32, 0.065, 0.75, 0.03, 2), &p.stripe),
        ],
        transform(vec3(0.0, 1.18, -0.5), Quat::IDENTITY, Vec3::ONE),
        Bone::Chest,
        transform(
            vec3(0.0, -0.14, -0.29),
            euler_deg(-80.0, 0.0, 0.0),
            vec3(0.55, 1.0, 0.55),
        ),
        0.34,
        vec3(0.0, 3.0, 0.0),
        1.0,
    );
    add(
        scene,
        "rear_window",
        vec![mesh(
            Geometry::rounded_box(1.44, 0.04, 0.84, 0.02, 2),
            &p.glass,
        )],
        transform(
            vec3(0.0, 1.09, -1.25),
            euler_deg(-15.0, 0.0, 0.0),
            Vec3::ONE,
        ),
        Bone::Pelvis,
        transform(
            vec3(0.0, 0.0, -0.22),
            euler_deg(-90.0, 0.0, 0.0),
            vec3(0.42, 1.0, 0.3),
        ),
        0.4,
        vec3(3.0, 0.0, 0.0),
        0.8,
    );
    add(
        scene,
        "deck",
        vec![mesh(
            Geometry::rounded_box(1.72, 0.06, 0.44, 0.03, 3),
            &p.paint,
        )],
        transform(vec3(0.0, 0.96, -1.86), Quat::IDENTITY, Vec3::ONE),
        Bone::Pelvis,
        transform(
            vec3(0.0, -0.04, 0.22),
            euler_deg(90.0, 0.0, 0.0),
            vec3(0.3, 1.0, 0.6),
        ),
        0.44,
        vec3(0.0, 0.0, 2.0),
        0.6,
    );

    // Rear bumper halves with the tail lights -> toe caps.
    for (side, name, ankle) in [
        (1.0f32, "tail_l", Bone::AnkleL),
        (-1.0, "tail_r", Bone::AnkleR),
    ] {
        let tail = Node::mesh(
            Geometry::rounded_box(0.5, 0.07, 0.05, 0.02, 2),
            p.taillight.clone(),
        );
        let parts = vec![
            mesh(Geometry::rounded_box(0.92, 0.58, 0.26, 0.08, 3), &p.paint),
            mesh(Geometry::rounded_box(0.6, 0.14, 0.04, 0.02, 1), &p.trim).at(
                side * 0.08,
                -0.15,
                -0.13,
            ),
            tail.at(side * 0.12, 0.13, -0.13),
        ];
        let node = add(
            scene,
            name,
            parts,
            transform(vec3(side * 0.46, 0.6, -2.06), Quat::IDENTITY, Vec3::ONE),
            ankle,
            transform(
                vec3(0.0, -0.06, 0.26),
                euler_deg(-20.0, 180.0, 0.0),
                vec3(0.27, 0.26, 0.5),
            ),
            0.5,
            vec3(0.0, side * 2.0, 0.0),
            0.7,
        );
        let tail = *scene[node].children().last().expect("tail light");
        glows.push((tail, Glow::Taillights, 1.0));
    }

    // Spoiler -> crest on the back.
    add(
        scene,
        "spoiler",
        vec![
            mesh(Geometry::rounded_box(1.7, 0.04, 0.3, 0.015, 2), &p.stripe),
            mesh(Geometry::rounded_box(0.05, 0.16, 0.12, 0.01, 1), &p.trim).at(0.55, -0.09, 0.0),
            mesh(Geometry::rounded_box(0.05, 0.16, 0.12, 0.01, 1), &p.trim).at(-0.55, -0.09, 0.0),
        ],
        transform(vec3(0.0, 1.2, -1.95), Quat::IDENTITY, Vec3::ONE),
        Bone::Chest,
        transform(
            vec3(0.0, 0.46, -0.3),
            euler_deg(-25.0, 0.0, 0.0),
            vec3(0.75, 1.0, 1.0),
        ),
        0.58,
        vec3(0.0, 4.0, 0.0),
        1.2,
    );

    // Doors -> back wings.
    for (side, name) in [(1.0f32, "door_l"), (-1.0, "door_r")] {
        // The side profile in (z, y), extruded along x. It is centered on the door, so the
        // door turns about its middle.
        let body = [
            vec2(-0.7, -0.44),
            vec2(0.72, -0.44),
            vec2(0.72, 0.18),
            vec2(-0.7, 0.2),
        ];
        let window = [
            vec2(-0.62, 0.23),
            vec2(0.62, 0.23),
            vec2(-0.1, 0.44),
            vec2(-0.6, 0.44),
        ];
        let to_side = |shape: &[Vec2], depth: f32| {
            Geometry::extrude(
                &shape.iter().map(|v| vec2(-v.x, v.y)).collect::<Vec<_>>(),
                depth,
                0.02,
            )
            .rotate_y(deg(90.0))
        };
        let parts = vec![
            mesh(to_side(&body, 0.06), &p.paint),
            mesh(to_side(&window, 0.03), &p.glass).at(-side * 0.04, 0.0, 0.0),
            mesh(Geometry::rounded_box(0.07, 0.025, 1.36, 0.01, 1), &p.chrome).at(
                side * 0.01,
                0.19,
                0.0,
            ),
        ];
        add(
            scene,
            name,
            parts,
            transform(vec3(side * 0.93, 0.72, 0.0), Quat::IDENTITY, Vec3::ONE),
            Bone::Chest,
            transform(
                vec3(side * 0.36, -0.42, -0.36),
                euler_deg(-14.0, side * -62.0, side * 38.0),
                Vec3::splat(0.62),
            ),
            0.0,
            vec3(0.0, side * 3.0, 0.0),
            0.6,
        );
    }

    // Fenders with the wheel arches -> shoulder pads and shins.
    let fender = |front: bool| {
        let (z0, z1, wz) = if front {
            (0.74, 2.02, 1.36)
        } else {
            (-2.04, -0.74, -1.36)
        };
        let mut shape = vec![vec2(z0, 0.3)];
        let r = 0.42;
        for i in 0..=14 {
            let a = std::f32::consts::PI * (1.0 - i as f32 / 14.0);
            shape.push(vec2(wz + r * a.cos(), 0.36 + r * a.sin()));
        }
        shape.push(vec2(z1, 0.3));
        shape.push(vec2(z1, if front { 0.8 } else { 0.94 }));
        shape.push(vec2(z0, if front { 0.92 } else { 0.94 }));
        let center = vec2(wz, 0.6);
        let shape: Vec<Vec2> = shape
            .iter()
            .map(|v| vec2(-(v.x - center.x), v.y - center.y))
            .collect();
        Geometry::extrude(&shape, 0.07, 0.025).rotate_y(deg(90.0))
    };
    let wheel = || {
        let tire = Geometry::lathe(
            &[
                vec2(0.2, -0.13),
                vec2(0.3, -0.14),
                vec2(0.35, -0.11),
                vec2(0.36, 0.0),
                vec2(0.35, 0.11),
                vec2(0.3, 0.14),
                vec2(0.2, 0.13),
            ],
            40,
        )
        .rotate_z(deg(90.0));
        let mut spokes = Vec::new();
        for i in 0..5 {
            let a = i as f32 / 5.0 * std::f32::consts::TAU;
            spokes.push(
                Geometry::rounded_box(0.05, 0.36, 0.04, 0.015, 1)
                    .translate(0.0, 0.1, 0.0)
                    .rotate_x(a),
            );
        }
        let rim = Geometry::merge(spokes).translate(0.11, 0.0, 0.0);
        vec![
            mesh(tire, &p.rubber),
            mesh(
                Geometry::cylinder(0.22, 0.22, 0.2, 40).rotate_z(deg(90.0)),
                &p.frame,
            ),
            mesh(rim, &p.chrome),
            mesh(
                Geometry::cylinder(0.05, 0.05, 0.25, 16).rotate_z(deg(90.0)),
                &p.chrome,
            ),
            mesh(Geometry::rounded_box(0.06, 0.14, 0.1, 0.02, 1), &p.caliper).at(0.07, 0.12, 0.1),
        ]
    };
    for (side, fender_name, wheel_name, shoulder) in [
        (1.0f32, "fender_fl", "wheel_fl", Bone::ShoulderL),
        (-1.0, "fender_fr", "wheel_fr", Bone::ShoulderR),
    ] {
        add(
            scene,
            fender_name,
            vec![mesh(fender(true), &p.paint)],
            transform(vec3(side * 0.9, 0.6, 1.36), Quat::IDENTITY, Vec3::ONE),
            shoulder,
            transform(
                vec3(side * 0.16, 0.0, 0.0),
                euler_deg(0.0, 0.0, side * 20.0) * Quat::from_rotation_y(deg(0.0)),
                vec3(0.62, 0.58, 0.48),
            ),
            0.24,
            vec3(side * 2.0, 0.0, 0.0),
            0.6,
        );
        let parts = mirrored(wheel(), side);
        add(
            scene,
            wheel_name,
            parts,
            transform(vec3(side * 0.8, 0.36, 1.36), Quat::IDENTITY, Vec3::ONE),
            shoulder,
            transform(
                vec3(side * 0.24, -0.02, 0.0),
                euler_deg(0.0, 0.0, side * 20.0),
                Vec3::splat(0.62),
            ),
            0.52,
            Vec3::ZERO,
            0.4,
        );
    }
    for (side, fender_name, wheel_name, knee) in [
        (1.0f32, "fender_rl", "wheel_rl", Bone::KneeL),
        (-1.0, "fender_rr", "wheel_rr", Bone::KneeR),
    ] {
        add(
            scene,
            fender_name,
            vec![mesh(fender(false), &p.paint)],
            transform(vec3(side * 0.9, 0.6, -1.36), Quat::IDENTITY, Vec3::ONE),
            knee,
            transform(
                vec3(side * 0.13, -0.24, -0.04),
                euler_deg(-90.0, 0.0, 0.0),
                vec3(0.6, 0.5, 0.5),
            ),
            0.36,
            vec3(side * 2.0, 0.0, 0.0),
            0.5,
        );
        let parts = mirrored(wheel(), side);
        add(
            scene,
            wheel_name,
            parts,
            transform(vec3(side * 0.8, 0.36, -1.36), Quat::IDENTITY, Vec3::ONE),
            knee,
            transform(
                vec3(side * 0.2, -0.3, -0.14),
                Quat::IDENTITY,
                Vec3::splat(0.6),
            ),
            0.48,
            Vec3::ZERO,
            0.4,
        );
    }

    // Side skirts -> forearm armor.
    for (side, name, elbow) in [
        (1.0f32, "skirt_l", Bone::ElbowL),
        (-1.0, "skirt_r", Bone::ElbowR),
    ] {
        add(
            scene,
            name,
            vec![mesh(
                Geometry::rounded_box(0.1, 0.14, 1.46, 0.04, 2),
                &p.trim,
            )],
            transform(vec3(side * 0.93, 0.22, 0.0), Quat::IDENTITY, Vec3::ONE),
            elbow,
            transform(
                vec3(side * 0.15, -0.24, 0.0),
                euler_deg(90.0, 0.0, 0.0),
                vec3(1.0, 1.4, 0.32),
            ),
            0.4,
            vec3(0.0, 0.0, side * 2.0),
            0.5,
        );
    }
    panels
}
