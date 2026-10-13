//! UNIT 01: thirteen parts glide in and snap together into a robot in a white void. The robot
//! is a `fframes_3d` scene on a transparent background, over an SVG backdrop.

// Colors are hex numbers, as in three.js: `0xff4a00`.
#![allow(clippy::unreadable_literal)]

use fframes::{
    AudioMap, AudioTimestamp, AudioTrack, Color, Duration, FFramesContext, Frame, Svgr, Video,
    include_media_dir, svgr,
};
use fframes_3d::math::{hash, progress, random_direction, smoothstep};
use fframes_3d::*;

include_media_dir!(pub struct AssemblyMedia, "examples/assembly/media");

pub const WIDTH: usize = 1920;
pub const HEIGHT: usize = 1080;
pub const FPS: usize = 60;
pub const DURATION: f32 = 11.0;

/// The snap time of each part, at 100 BPM. The first parts snap on every beat, then on every
/// half beat. The head snaps on a beat of its own.
pub const SNAPS: [f32; 13] = [
    1.8, 2.4, 3.0, 3.6, 4.2, 4.8, 5.1, 5.4, 5.7, 6.0, 6.3, 6.6, 7.2,
];
/// The time that a part travels before it snaps.
const TRAVEL: f32 = 1.5;
/// The orange line across the visor lights up.
pub const ACCENT: f32 = 7.8;
/// The head turns to the camera.
pub const TURN: f32 = 8.1;
/// The title comes in.
pub const TITLE: f32 = 8.4;

const MONO: &str = "IBM Plex Mono";
const INK: &str = "#161616";
const ORANGE: &str = "#ff4a00";

/// A part of the robot and its path.
struct Part {
    node: NodeId,
    name: &'static str,
    position: Vec3,
    rotation: Quat,
    /// The start position, relative to the place of the part.
    from: Vec3,
    /// The start rotation, relative to the final rotation.
    spin: Quat,
}

/// The film: the 3D scene with the parts and the renderer.
pub struct Assembly {
    scene: Scene,
    parts: Vec<Part>,
    head: NodeId,
    accent: NodeId,
    renderer: Renderer,
}

impl std::fmt::Debug for Assembly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Assembly").finish_non_exhaustive()
    }
}

/// A bright, even photo studio. The chrome reflects its soft gradients.
fn environment() -> Environment {
    Environment::Studio(Studio {
        sky: Rgb::hex(0xf4f4f2),
        horizon: Rgb::hex(0xd8d8d4),
        ground: Rgb::hex(0x8a8a86),
        softboxes: vec![
            SoftBox::new(vec3(-0.5, 0.8, 0.6), 60.0, 40.0, Rgb::WHITE * 2.5),
            SoftBox::new(vec3(1.0, 0.3, -0.2), 12.0, 70.0, Rgb::WHITE * 2.0),
            SoftBox::new(vec3(0.0, 0.2, -1.0), 90.0, 6.0, Rgb::WHITE * 1.4),
        ],
        intensity: 0.85,
        rotation: 0.0,
    })
}

/// Cubic ease in and out.
fn ease_in_out(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x < 0.5 {
        4.0 * x * x * x
    } else {
        1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
    }
}

/// Splits the travel progress `x` into a slow glide and a fast snap, each from 0 to 1. The
/// glide stops near the place of the part. The snap then pulls the part in.
fn travel(x: f32) -> (f32, f32) {
    const GLIDE: f32 = 0.78;
    if x < GLIDE {
        (ease_in_out(x / GLIDE), 0.0)
    } else {
        let s = (x - GLIDE) / (1.0 - GLIDE);
        (1.0, s * s * s)
    }
}

/// Exponential decay from `at`, 0 before.
fn after(t: f32, at: f32, rate: f32) -> f32 {
    if t < at {
        0.0
    } else {
        (-(t - at) * rate).exp()
    }
}

impl Assembly {
    pub fn new() -> Self {
        let ceramic = Material::standard(0xf2f2ef).roughness(0.55).clearcoat(0.3);
        let graphite = Material::metal(0x2a2b2e, 0.38);
        let chrome = Material::metal(0xe2e5e9, 0.14);

        let mut scene = Scene::new().environment(environment());
        scene.add(
            Node::mesh(Geometry::floor(40.0, 40.0), Material::shadow_catcher(0.32))
                .shadows(false, true),
        );
        scene.add(Node::light(Light::hemisphere(0xffffff, 0x9a9690, 2.2)));
        scene.add(Node::light(
            Light::sun(Rgb::hex(0xfffaf2), 2.6, vec3(2.5, 9.0, 4.0)).with_shadow(Shadow {
                map_size: 2048,
                softness: 5.0,
                strength: 1.0,
                ..Default::default()
            }),
        ));

        let mut parts = Vec::new();
        // Adds a part. The part stays hidden until it starts to travel.
        let mut add = |scene: &mut Scene, name: &'static str, node: Node, from: Vec3| -> NodeId {
            let (position, rotation) = (node.position, node.rotation);
            let id = scene.add(node.hidden());
            let i = parts.len() as u32;
            let spin =
                Quat::from_axis_angle(random_direction(i * 7 + 3), deg(70.0 + 60.0 * hash(i)));
            parts.push(Part {
                node: id,
                name,
                position,
                rotation,
                from,
                spin,
            });
            id
        };
        // A start position at the side of the robot, out of the frame.
        let side = |s: f32, up: f32, z: f32| vec3(s * 4.2, up, z);

        // The robot is slender: 2.3 m tall, with long limbs and a tapered torso.
        for s in [1.0f32, -1.0] {
            let foot = Geometry::extrude(
                &[
                    vec2(-0.1, 0.0),
                    vec2(0.2, 0.0),
                    vec2(0.2, 0.04),
                    vec2(0.02, 0.09),
                    vec2(-0.1, 0.09),
                ],
                0.13,
                0.03,
            )
            .rotate_y(deg(-90.0));
            add(
                &mut scene,
                if s > 0.0 { "FOOT L" } else { "FOOT R" },
                Node::mesh(foot, graphite.clone()).at(s * 0.13, 0.0, 0.0),
                side(s, 0.6, 1.0),
            );
        }
        for s in [1.0f32, -1.0] {
            let id = add(
                &mut scene,
                if s > 0.0 { "SHIN L" } else { "SHIN R" },
                Node::group().at(s * 0.13, 0.36, 0.0),
                side(s, 1.4, -0.6),
            );
            scene.add_to(
                id,
                Node::mesh(Geometry::sphere(0.055, 32, 16), chrome.clone()).at(0.0, -0.27, 0.0),
            );
            scene.add_to(
                id,
                Node::mesh(Geometry::capsule(0.06, 0.4, 8, 40), ceramic.clone()).at(0.0, 0.02, 0.0),
            );
        }
        for s in [1.0f32, -1.0] {
            let id = add(
                &mut scene,
                if s > 0.0 { "THIGH L" } else { "THIGH R" },
                Node::group().at(s * 0.13, 0.88, 0.0),
                side(s, 2.2, 0.8),
            );
            scene.add_to(
                id,
                Node::mesh(Geometry::sphere(0.064, 32, 16), chrome.clone()).at(0.0, -0.27, 0.0),
            );
            scene.add_to(
                id,
                Node::mesh(Geometry::capsule(0.078, 0.38, 8, 40), ceramic.clone())
                    .at(0.0, 0.03, 0.0),
            );
        }
        add(
            &mut scene,
            "PELVIS",
            Node::mesh(
                Geometry::rounded_box(0.34, 0.14, 0.2, 0.06, 5),
                graphite.clone(),
            )
            .at(0.0, 1.2, 0.0),
            vec3(0.0, 0.4, -4.5),
        );
        let torso = add(
            &mut scene,
            "TORSO",
            Node::group().at(0.0, 1.28, 0.0),
            vec3(0.0, 3.6, 0.0),
        );
        let profile = [
            vec2(0.0, 0.0),
            vec2(0.1, 0.005),
            vec2(0.13, 0.05),
            vec2(0.15, 0.14),
            vec2(0.21, 0.32),
            vec2(0.245, 0.45),
            vec2(0.25, 0.52),
            vec2(0.23, 0.58),
            vec2(0.15, 0.63),
            vec2(0.0, 0.645),
        ];
        scene.add_to(
            torso,
            Node::mesh(Geometry::lathe(&profile, 96), ceramic.clone()).scaled3(1.0, 1.0, 0.62),
        );
        scene.add_to(
            torso,
            Node::mesh(
                Geometry::rounded_box(0.12, 0.01, 0.01, 0.004, 2),
                graphite.clone(),
            )
            .at(0.0, 0.3, 0.128),
        );
        scene.add_to(
            torso,
            Node::mesh(Geometry::cylinder(0.035, 0.04, 0.12, 32), chrome.clone())
                .at(0.0, 0.68, 0.0),
        );
        for s in [1.0f32, -1.0] {
            let id = add(
                &mut scene,
                if s > 0.0 { "ARM L" } else { "ARM R" },
                Node::group().at(s * 0.33, 1.56, 0.0).rotate_z(s * deg(6.0)),
                side(s, 2.6, -0.4),
            );
            scene.add_to(
                id,
                Node::mesh(Geometry::sphere(0.06, 32, 16), chrome.clone()).at(0.0, 0.19, 0.0),
            );
            scene.add_to(
                id,
                Node::mesh(
                    Geometry::sphere_partial(0.085, 48, 24, 0.0, deg(100.0)),
                    graphite.clone(),
                )
                .at(0.0, 0.21, 0.0),
            );
            scene.add_to(
                id,
                Node::mesh(Geometry::capsule(0.056, 0.3, 8, 40), ceramic.clone())
                    .at(0.0, -0.04, 0.0),
            );
        }
        for s in [1.0f32, -1.0] {
            let fore = Node::group()
                .at(s * 0.36, 1.12, 0.03)
                .rotate_z(s * deg(3.0))
                .rotate_x(deg(-5.0));
            let id = add(
                &mut scene,
                if s > 0.0 { "FOREARM L" } else { "FOREARM R" },
                fore,
                side(s, 0.9, 1.2),
            );
            scene.add_to(
                id,
                Node::mesh(Geometry::sphere(0.048, 32, 16), chrome.clone()).at(0.0, 0.2, 0.0),
            );
            scene.add_to(
                id,
                Node::mesh(Geometry::capsule(0.05, 0.28, 8, 40), graphite.clone()),
            );
            scene.add_to(
                id,
                Node::mesh(Geometry::capsule(0.045, 0.06, 8, 32), ceramic.clone())
                    .at(0.0, -0.25, 0.01),
            );
        }
        let head = add(
            &mut scene,
            "HEAD",
            Node::group().at(0.0, 2.1, 0.0),
            vec3(0.0, 4.0, 0.6),
        );
        scene.add_to(
            head,
            Node::mesh(Geometry::sphere(1.0, 64, 32), ceramic.clone()).scaled3(0.13, 0.17, 0.15),
        );
        scene.add_to(
            head,
            Node::mesh(
                Geometry::rounded_box(0.235, 0.055, 0.15, 0.027, 5),
                graphite.clone(),
            )
            .at(0.0, 0.02, 0.085),
        );
        let accent = scene.add_to(
            head,
            Node::mesh(
                Geometry::rounded_box(0.16, 0.006, 0.01, 0.003, 2),
                Material::standard(0x1a1a1a).emissive(0xff4a00, 0.0),
            )
            .at(0.0, 0.02, 0.161),
        );

        Self {
            scene,
            parts,
            head,
            accent,
            renderer: Renderer::new(WIDTH as u32, HEIGHT as u32)
                .tone_mapping(ToneMapping::Neutral)
                .exposure(1.05)
                .bloom(Bloom {
                    strength: 0.5,
                    radius: 0.4,
                    threshold: 1.3,
                }),
        }
    }

    /// The number of parts in place at `t`.
    pub fn assembled(t: f32) -> usize {
        SNAPS.iter().filter(|&&s| s <= t).count()
    }

    /// The posed scene and the camera at `t`.
    pub fn scene_at(&self, t: f32) -> (Scene, Camera) {
        let mut scene = self.scene.clone();
        // Each snap pushes the whole body down a little.
        let settle: f32 = SNAPS
            .iter()
            .map(|&s| after(t, s, 18.0) * (t - s).clamp(0.0, 0.2) * 0.02)
            .sum();
        for (i, part) in self.parts.iter().enumerate() {
            let start = SNAPS[i] - TRAVEL;
            let (glide, snap) = travel(progress(t, start, SNAPS[i]));
            let hover = 0.18 * (1.0 - snap);
            let offset =
                part.from * (1.0 - glide) + part.from.normalize_or(Vec3::Y) * hover * glide;
            let node = &mut scene[part.node];
            node.visible = t >= start;
            node.position = part.position + offset - Vec3::Y * settle;
            node.rotation = part.spin.slerp(Quat::IDENTITY, glide) * part.rotation;
        }
        // The head turns to the camera and the visor line lights up.
        let turn = ease_in_out(progress(t, TURN, TURN + 0.7));
        scene[self.head].rotation = Quat::from_rotation_y(deg(-16.0) * turn)
            * Quat::from_rotation_x(deg(-4.0) * turn)
            * scene[self.head].rotation;
        let flicker = if (ACCENT..ACCENT + 0.18).contains(&t) {
            if hash(((t - ACCENT) * 50.0) as u32 + 3) > 0.45 {
                1.0
            } else {
                0.2
            }
        } else {
            1.0
        };
        scene[self.accent]
            .material_mut()
            .expect("accent")
            .emissive_intensity = 6.0 * smoothstep(ACCENT, ACCENT + 0.05, t) * flicker;

        // One slow orbit, and a slow push in at the end.
        let x = t / DURATION;
        let push = ease_in_out(progress(t, TURN, DURATION));
        let camera = Camera::perspective(28.0)
            .orbit(
                vec3(0.0, 1.14 + 0.1 * push, 0.0),
                5.9 - 0.7 * x - 0.3 * push,
                -38.0 + 50.0 * ease_in_out(x),
                7.0 + 4.0 * x,
            )
            .shake(
                t,
                SNAPS.iter().map(|&s| after(t, s, 25.0)).sum::<f32>() * 0.0012,
                4,
            );
        (scene, camera)
    }

    /// The counter, the name of the last part, the title and the fades.
    fn overlay(&self, t: f32) -> Svgr<'static> {
        let n = Self::assembled(t);
        let last = if n == 0 { "" } else { self.parts[n - 1].name };
        let ui = smoothstep(0.4, 1.0, t) * (1.0 - smoothstep(TITLE - 0.3, TITLE, t));
        let title = smoothstep(TITLE, TITLE + 0.6, t);
        let spacing = 40.0 - 22.0 * ease_in_out(progress(t, TITLE, TITLE + 1.2));
        let bar = n as f32 / SNAPS.len() as f32 * 240.0;
        let fade = smoothstep(DURATION - 0.4, DURATION, t);
        let fade_in = 1.0 - smoothstep(0.0, 0.6, t);
        svgr!(
            <g>
                <g opacity={ui} font-family={MONO} font-weight="600" font-size="14" letter-spacing="4" fill={INK}>
                    <text x="96" y="104">"UNIT 01"</text>
                    <text x="96" y="128" opacity="0.45">"ASSEMBLY SEQUENCE"</text>
                    <text x="1824" y="104" text-anchor="end">{format!("{n:02} / {:02}", SNAPS.len())}</text>
                    <text x="1824" y="128" text-anchor="end" opacity="0.45">{last.to_owned()}</text>
                    <rect x="1584" y="972" width="240" height="1" fill={INK} opacity="0.2" />
                    <rect x="1584" y="972" width={bar.max(0.01)} height="1" fill={INK} />
                </g>
                <g opacity={title} font-family={MONO} fill={INK}>
                    <text x="96" y="940" font-weight="600" font-size="34" letter-spacing={spacing}>"UNIT 01"</text>
                    <rect x="98" y="962" width="44" height="2" fill={ORANGE} />
                    <text x="96" y="1000" font-weight="400" font-size="13" letter-spacing="6" opacity="0.5">"ASSEMBLED IN FFRAMES-3D"</text>
                </g>
                <rect x="0" y="0" width="1920" height="1080" fill="#f4f4f2" opacity={fade_in.max(fade)} />
            </g>
        )
    }
}

impl Default for Assembly {
    fn default() -> Self {
        Self::new()
    }
}

impl Video for Assembly {
    const FPS: usize = FPS;
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;
    const BACKGROUND_COLOR: Color = Color::WHITE;

    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(DURATION)
    }

    fn audio(&self) -> AudioMap<'_> {
        use AudioTimestamp::{Eof, Second};
        let mut tracks = vec![
            AudioTrack::new("drone.wav", Second(0.0)..Eof)
                .gain_db(-8.0)
                .fade_out(1.0),
        ];
        for (i, &snap) in SNAPS.iter().enumerate() {
            let pan = (self.parts[i].from.x / 4.2).clamp(-1.0, 1.0) * 0.5;
            tracks.push(
                AudioTrack::new("air.wav", Second(snap - 0.55)..Eof)
                    .gain_db(-20.0)
                    .pan(pan),
            );
            let file = if i == SNAPS.len() - 1 {
                "snap_head.wav"
            } else if i % 2 == 0 {
                "snap1.wav"
            } else {
                "snap2.wav"
            };
            tracks.push(
                AudioTrack::new(file, Second(snap)..Eof)
                    .gain_db(-4.0)
                    .pan(pan * 0.6),
            );
        }
        tracks.push(AudioTrack::new("chime.wav", Second(ACCENT)..Eof).gain_db(-6.0));
        AudioMap::from(tracks)
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let (scene, camera) = self.scene_at(t);
        let image = self.renderer.render(&scene, &camera);
        let overlay = self.overlay(t);
        svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1920 1080" width={WIDTH} height={HEIGHT}>
                <defs>
                    <radialGradient id="void" cx="0.5" cy="0.45" r="0.8">
                        <stop offset="0" stop-color="#fbfbfa" />
                        <stop offset="0.6" stop-color="#efefec" />
                        <stop offset="1" stop-color="#d6d6d2" />
                    </radialGradient>
                </defs>
                <rect x="0" y="0" width="1920" height="1080" fill="url(#void)" />
                <image href={image.href()} x="0" y="0" width="1920" height="1080" />
                {overlay}
            </svg>
        )
    }
}
