//! OVERDRIVE: a sports car jumps and transforms into a robot in the air. The robot fires an
//! arm cannon, then turns back into the car and drives away.

// Colors are hex numbers, as in three.js: `0xff8800`.
#![allow(clippy::unreadable_literal)]

pub mod hud;
pub mod robot;
pub mod stage;
pub mod story;

use fframes::{
    AudioMap, AudioTimestamp, AudioTrack, Color, Duration, FFramesContext, Frame, Svgr, Video,
    include_media_dir,
};
use fframes_3d::math::{hash, random_direction, smoothstep};
use fframes_3d::*;
use robot::{Palette, Robot, RobotState};
use story::*;

include_media_dir!(pub struct OverdriveMedia, "examples/overdrive/media");

pub const WIDTH: usize = 1920;
pub const HEIGHT: usize = 1080;
pub const FPS: usize = 60;

/// The effect nodes in the template scene. [`Show::effects`] animates them for each frame.
struct Fx {
    shock: NodeId,
    impact_light: NodeId,
    muzzle_flash: NodeId,
    muzzle_light: NodeId,
    beam: NodeId,
    beam_core: NodeId,
    cones: [NodeId; 2],
    debris: Vec<(NodeId, Vec3, Vec3)>,
}

/// The film. The 3D world is built once, then cloned and posed for each frame.
pub struct Show {
    pub scene: Scene,
    pub robot: Robot,
    pub stage: stage::Stage,
    calibration: Calibration,
    fx: Fx,
    sparks: Vec<SparkBurst>,
    pub renderer: Renderer,
}

/// The posed scene, the camera and the robot state at one moment.
pub struct Shot {
    pub scene: Scene,
    pub camera: Camera,
    pub state: RobotState,
}

impl Show {
    pub fn new() -> Self {
        let mut scene = Scene::new();
        let stage = stage::build(&mut scene);
        let robot = Robot::build(&mut scene, &Palette::new());
        let calibration = Calibration::measure(&robot, &scene);
        let fx = Self::build_fx(&mut scene);
        let mut show = Self {
            scene,
            robot,
            stage,
            calibration,
            fx,
            sparks: Vec::new(),
            renderer: Renderer::new(WIDTH as u32, HEIGHT as u32)
                .exposure(1.25)
                .bloom(Bloom {
                    strength: 0.85,
                    radius: 0.55,
                    threshold: 1.0,
                }),
        };
        show.sparks = show.spark_bursts();
        show
    }

    /// Adds the effect nodes to `scene`.
    fn build_fx(scene: &mut Scene) -> Fx {
        let shock = scene.add(
            Node::mesh(
                Geometry::ring(0.86, 1.0, 128).rotate_x(deg(-90.0)),
                Material::glow(stage::CYAN, 0.0).double_sided(),
            )
            .shadows(false, false)
            .at(0.0, 0.04, 0.4),
        );
        let impact_light =
            scene.add(Node::light(Light::point(stage::CYAN, 0.0, 0.0)).at(0.0, 0.7, 0.9));
        let muzzle_flash = scene.add(
            Node::mesh(Geometry::sphere(1.0, 24, 12), Material::glow(0xbff6ff, 0.0))
                .shadows(false, false),
        );
        let muzzle_light = scene.add(Node::light(Light::point(stage::CYAN, 0.0, 0.0)));
        let beam = scene.add(
            Node::mesh(
                Geometry::cylinder(1.0, 1.0, 1.0, 24),
                Material::glow(stage::CYAN, 0.0).double_sided(),
            )
            .shadows(false, false),
        );
        let beam_core = scene.add(
            Node::mesh(
                Geometry::cylinder(1.0, 1.0, 1.0, 16),
                Material::glow(0xffffff, 0.0),
            )
            .shadows(false, false),
        );
        let mut cone = Geometry::cone(1.5, 10.0, 32)
            .translate(0.0, -5.0, 0.0)
            .rotate_x(deg(-90.0));
        cone.colors = cone
            .positions
            .iter()
            .map(|p| Vec4::new(1.0, 1.0, 1.0, (1.0 - p.z / 10.0).clamp(0.0, 1.0).powi(2)))
            .collect();
        let cone_material = Material::glow(0xcfe4ff, 0.0).double_sided();
        let cones = [
            scene.add(Node::mesh(cone.clone(), cone_material.clone()).shadows(false, false)),
            scene.add(Node::mesh(cone, cone_material).shadows(false, false)),
        ];
        let rock = Material::standard(0x1a1b20).roughness(0.8).flat();
        let debris = (0..18)
            .map(|i| {
                let size = 0.06 + 0.1 * hash(i * 13 + 1);
                let node =
                    scene.add(Node::mesh(Geometry::icosahedron(size, 0), rock.clone()).hidden());
                let dir = random_direction(i * 31 + 7);
                let velocity = vec3(dir.x * 6.0, 3.0 + 6.0 * hash(i * 3), dir.z * 6.0 + 2.0);
                (
                    node,
                    velocity,
                    random_direction(i * 17 + 3) * (6.0 + 10.0 * hash(i)),
                )
            })
            .collect();
        Fx {
            shock,
            impact_light,
            muzzle_flash,
            muzzle_light,
            beam,
            beam_core,
            cones,
            debris,
        }
    }

    /// The spark bursts: one for each panel that locks into place, one at the landing and
    /// one at the muzzle.
    fn spark_bursts(&self) -> Vec<SparkBurst> {
        let mut bursts = Vec::new();
        for (i, (time, panel)) in panel_locks(&self.robot).into_iter().enumerate() {
            let position = self.pose(time).scene[self.robot.panels[panel].node].position;
            let before = self.pose(time - 0.01).scene[self.robot.panels[panel].node].position;
            bursts.push(SparkBurst {
                time,
                position,
                velocity: (position - before) / 0.01 * 0.8,
                count: 10,
                speed: 5.0,
                seed: i as u32 + 1,
                color: Rgb::hex(0xffa040),
            });
        }
        bursts.push(SparkBurst {
            time: LAND,
            position: vec3(0.0, 0.1, 0.6),
            velocity: Vec3::ZERO,
            count: 90,
            speed: 9.0,
            seed: 900,
            color: Rgb::hex(0xffb050),
        });
        let muzzle = self.pose(FIRE).scene.world_position(self.robot.muzzle);
        bursts.push(SparkBurst {
            time: FIRE,
            position: muzzle,
            velocity: Vec3::ZERO,
            count: 50,
            speed: 7.0,
            seed: 950,
            color: Rgb::hex(0x9ff4ff),
        });
        bursts
    }

    /// The robot and the camera at `t`, without effects.
    fn pose(&self, t: f32) -> Shot {
        let (mut state, grounded) = robot_at(t, &self.calibration);
        let mut scene = self.scene.clone();
        if grounded {
            let lift = self
                .robot
                .ground_offset(&mut scene, state.root, &state.pose);
            state.root = Mat4::from_translation(vec3(0.0, lift, 0.0)) * state.root;
        }
        self.robot.apply(&mut scene, &state);
        let camera = camera_at(t, &state);
        Shot {
            scene,
            camera,
            state,
        }
    }

    /// The robot, the camera and the effects at `t`.
    pub fn shot(&self, t: f32) -> Shot {
        let mut shot = self.pose(t);
        self.effects(t, &mut shot);
        shot
    }

    fn effects(&self, t: f32, shot: &mut Shot) {
        let fx = &self.fx;
        let scene = &mut shot.scene;
        let state = &shot.state;

        // Lights that follow the action.
        let hero = smoothstep(LAND - 0.1, LAND + 0.4, t) * (1.0 - smoothstep(LEAP, LEAP + 0.5, t));
        *scene[self.stage.rim_cyan]
            .light_mut()
            .expect("light")
            .intensity_mut() = 70.0 * hero;
        *scene[self.stage.rim_magenta]
            .light_mut()
            .expect("light")
            .intensity_mut() = 60.0 * hero;
        let headlights = state.headlights / 7.0 * (1.0 - state.morph);
        let headlight = &mut scene[self.stage.headlight];
        headlight.position = state.car.transform_point3(vec3(0.0, 0.62, 2.3));
        headlight.rotation = state.car.to_scale_rotation_translation().1;
        *headlight.light_mut().expect("light").intensity_mut() = 30.0 * headlights;
        for (i, &cone) in fx.cones.iter().enumerate() {
            let side = if i == 0 { 0.6 } else { -0.6 };
            let node = &mut scene[cone];
            node.position = state.car.transform_point3(vec3(side, 0.64, 2.2));
            node.rotation =
                state.car.to_scale_rotation_translation().1 * Quat::from_rotation_x(deg(5.0));
            node.visible = headlights > 0.01;
            node.material_mut().expect("cone").color = Rgb::hex(0xcfe4ff) * (0.022 * headlights);
        }

        // The landing: a shock wave ring, a flash of light and debris.
        let age = t - LAND;
        let ring = &mut scene[fx.shock];
        ring.visible = (0.0..1.0).contains(&age);
        let r = 0.5 + 16.0 * (1.0 - (-age.max(0.0) * 3.5).exp());
        ring.scale = vec3(r, 1.0, r);
        ring.material_mut().expect("ring").color =
            Rgb::hex(stage::CYAN) * (14.0 * (1.0 - age).max(0.0).powi(2));
        *scene[fx.impact_light]
            .light_mut()
            .expect("light")
            .intensity_mut() = 450.0 * impact(t, LAND, 8.0);
        for &(node, velocity, spin) in &fx.debris {
            let n = &mut scene[node];
            n.visible = age > 0.0;
            if age > 0.0 {
                let a = age.min(1.6);
                let mut p =
                    vec3(0.0, 0.15, 0.6) + velocity * a + vec3(0.0, -9.8, 0.0) * (0.5 * a * a);
                p.y = p.y.max(0.06);
                n.position = p;
                n.rotation = Quat::from_scaled_axis(spin * a.min(1.2));
            }
        }

        // The cannon: the charge glow, the shot and the beam.
        let muzzle = scene.world_matrix(self.robot.muzzle);
        let muzzle_pos = muzzle.w_axis.truncate();
        let dir = (-muzzle.y_axis.truncate()).normalize_or(Vec3::Z);
        let charge = state.charge;
        let flash = &mut scene[fx.muzzle_flash];
        flash.position = muzzle_pos;
        let flash_size = 0.08 * (charge / 10.0).min(1.0)
            + 0.35 * impact(t, FIRE, 12.0) * smoothstep(FIRE - 0.01, FIRE, t);
        flash.scale = Vec3::splat(flash_size.max(0.001));
        flash.visible = flash_size > 0.002;
        flash.material_mut().expect("flash").color = Rgb::hex(0xbff6ff) * (4.0 + charge * 0.5);
        let light = &mut scene[fx.muzzle_light];
        light.position = muzzle_pos + dir * 0.3;
        *light.light_mut().expect("light").intensity_mut() =
            4.0 * charge + 140.0 * impact(t, FIRE, 10.0) * smoothstep(FIRE - 0.01, FIRE, t);
        let beam_on =
            smoothstep(FIRE - 0.01, FIRE, t) * (1.0 - smoothstep(FIRE + 0.12, FIRE + 0.45, t));
        let length = 80.0;
        for (node, radius, color) in [
            (fx.beam, 0.16, Rgb::hex(stage::CYAN) * 6.0),
            (fx.beam_core, 0.05, Rgb::WHITE * 14.0),
        ] {
            let n = &mut scene[node];
            n.visible = beam_on > 0.0;
            n.position = muzzle_pos + dir * (length * 0.5);
            n.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
            n.scale = vec3(
                radius * (0.4 + 0.6 * beam_on),
                length,
                radius * (0.4 + 0.6 * beam_on),
            );
            n.material_mut().expect("beam").color = color * beam_on;
        }

        // Particles: sparks, the charge, rain and dust.
        let mut glow = sparks_at(t, &self.sparks);
        if (CHARGE..FIRE).contains(&t) {
            let (u_axis, v_axis) = dir.any_orthonormal_pair();
            for i in 0..70u32 {
                let u = ((t - CHARGE) * 1.6 + hash(i)).fract();
                let radius = 1.8 * (1.0 - u).powi(2);
                let angle = i as f32 * 2.4 + u * 9.0;
                let p = muzzle_pos
                    + (u_axis * angle.cos() + v_axis * angle.sin()) * radius
                    + dir * (1.0 - u) * 0.9;
                glow.push(
                    Particle::new(p, 0.012, Rgb::hex(0x7ff0ff) * (3.0 + 12.0 * u))
                        .tail((muzzle_pos - p) * 0.08),
                );
            }
        }
        let camera = shot.camera.position;
        let rain_box = 22.0;
        let rain_height = 12.0;
        for i in 0..900u32 {
            let base = vec3(
                hash(i * 3) * rain_box,
                hash(i * 3 + 1) * rain_height,
                hash(i * 3 + 2) * rain_box,
            );
            let x = camera.x + (base.x - camera.x).rem_euclid(rain_box) - rain_box * 0.5;
            let z = camera.z + (base.z - camera.z).rem_euclid(rain_box) - rain_box * 0.5;
            let y = (base.y - t * 16.0).rem_euclid(rain_height);
            let drop = vec3(x, y, z);
            // Drops near the lens would be large smears, so they fade out.
            let near = smoothstep(2.0, 4.0, drop.distance(camera));
            if near > 0.0 {
                glow.push(
                    Particle::new(drop, 0.005, Rgb::hex(0xa8c8ff) * (0.35 * near))
                        .tail(vec3(0.15, 0.45, 0.0)),
                );
            }
        }
        scene.add(Node::particles(glow, Blending::Additive));

        if (0.0..1.8).contains(&age) {
            let dust: Vec<Particle> = (0..50u32)
                .map(|i| {
                    let dir = random_direction(i * 7 + 400);
                    let flat = vec3(dir.x, 0.0, dir.z).normalize_or(Vec3::X);
                    let reach = (5.0 + 4.0 * hash(i)) * (1.0 - (-age * 2.5).exp());
                    let p = vec3(0.0, 0.25 + 0.6 * age * hash(i * 5), 0.6) + flat * reach;
                    Particle::new(p, 0.35 + 0.5 * age, 0x6a6878).opacity(0.22 * (1.0 - age / 1.8))
                })
                .collect();
            scene.add(Node::particles(dust, Blending::Normal));
        }
    }

    /// The number of motion blur samples at `t`. Fast shots get three.
    fn blur_samples(t: f32) -> usize {
        if t < LAND + 0.25 || (LEAP - 0.05..TITLE).contains(&t) {
            3
        } else {
            1
        }
    }

    /// The rendered 3D image of the frame at `t`.
    pub fn image(&self, t: f32) -> fframes::media::ImageData<'static> {
        let samples = Self::blur_samples(t);
        // The shutter is open for half a frame: 1/120 s at 60 fps.
        let shutter = 0.5 / FPS as f32;
        self.renderer.render_motion_blur(samples, |offset| {
            let shot = self.shot(t + offset * shutter);
            (shot.scene, shot.camera)
        })
    }
}

impl Default for Show {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Show {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Show").finish_non_exhaustive()
    }
}

/// The sound file, the level in dB and the pan of an event.
fn effect_for(kind: EventKind, index: usize) -> (&'static str, f32, f32) {
    const CLANKS: [&str; 4] = ["clank1.wav", "clank2.wav", "clank3.wav", "clank4.wav"];
    match kind {
        EventKind::Clank => (
            CLANKS[index % 4],
            -9.0,
            if index.is_multiple_of(2) { -0.35 } else { 0.35 },
        ),
        EventKind::Launch => ("launch.wav", -4.0, 0.0),
        EventKind::Land => ("impact.wav", -1.0, 0.0),
        EventKind::PowerUp => ("powerup.wav", -6.0, 0.0),
        EventKind::Blip => (
            "blip.wav",
            -14.0,
            if index.is_multiple_of(2) { -0.5 } else { 0.5 },
        ),
        EventKind::CannonOpen => ("cannon_open.wav", -6.0, -0.2),
        EventKind::Charge => ("charge.wav", -8.0, -0.1),
        EventKind::Fire => ("blast.wav", -1.5, 0.0),
        EventKind::Leap => ("launch.wav", -6.0, 0.0),
        EventKind::CarLand => ("thud.wav", -5.0, 0.0),
        EventKind::PassBy => ("passby.wav", -3.0, 0.0),
        EventKind::Title => ("title.wav", -3.0, 0.0),
    }
}

impl Video for Show {
    const FPS: usize = FPS;
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(DURATION)
    }

    fn audio(&self) -> AudioMap<'_> {
        use AudioTimestamp::{Eof, Second};
        let mut tracks = vec![
            AudioTrack::new("music.wav", Second(0.0)..Eof).gain_db(-5.0),
            AudioTrack::new("engine.wav", Second(0.0)..Second(LAUNCH + 0.4))
                .gain_db(-7.0)
                .fade_out(0.4),
            AudioTrack::new("servo.wav", Second(MORPH_START)..Second(MORPH_END + 0.1))
                .gain_db(-12.0)
                .fade_out(0.2),
            AudioTrack::new(
                "servo.wav",
                Second(REMORPH_START)..Second(REMORPH_END + 0.1),
            )
            .gain_db(-13.0)
            .fade_out(0.2),
        ];
        // The effects come from the event list of the animation, so they stay in sync.
        for (i, event) in events(&self.robot).into_iter().enumerate() {
            let (file, gain, pan) = effect_for(event.kind, i);
            tracks.push(
                AudioTrack::new(file, Second(event.time)..Eof)
                    .gain_db(gain)
                    .pan(pan),
            );
        }
        AudioMap::from(tracks)
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let image = self.image(t);
        let shot = self.shot(t);
        let overlay = hud::overlay(t, &shot, self);
        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1920 1080" width={WIDTH} height={HEIGHT}>
                <image href={image.href()} x="0" y="0" width="1920" height="1080" />
                {overlay}
            </svg>
        )
    }
}
