//! FFRAMES, ASSEMBLED: a short film about a lab robot with a camera eye. It spells
//! `fframes` with letter pieces, throws the "a" across the floor by mistake, and fixes it.

// Colors are hex numbers, as in three.js: `0xff4a00`.
#![allow(clippy::unreadable_literal)]

pub mod arm;
pub mod effects;
pub mod lab;
pub mod story;

use fframes::three_d::math::smoothstep;
use fframes::three_d::*;
use fframes::{
    AudioMap, AudioTimestamp, AudioTrack, Color, Duration, FFramesContext, Frame, Svgr, Video,
    include_media_dir, svgr,
};

use crate::arm::{Arm, Expression, Joints, Palette};
use crate::effects::{hash, noise, progress};
use crate::story::{Phase, Sound, Story};

include_media_dir!(pub struct ArmMedia, "examples/arm/media");

pub const WIDTH: usize = 1920;
pub const HEIGHT: usize = 1080;
pub const FPS: usize = 60;

/// The font of the letter pieces and of the lettering on the arm.
pub const FONT: &[u8] = include_bytes!("../media/Inter_18pt-ExtraBold.ttf");
/// The word that the robot spells.
pub const WORD: &str = "fframes";
/// Z of the row where the robot spells the word.
pub const WORD_Z: f32 = 1.25;
/// Z of the rack where the letter pieces wait.
pub const RACK_Z: f32 = -1.3;

const MONO: &str = "IBM Plex Mono";
const INK: &str = "#1c1d20";
const ORANGE: &str = "#ff4a00";

/// The part of a frame that the shutter is open in the shots with motion blur (180°).
const SHUTTER: f32 = 0.5;

/// The film: the 3D scene, the story and the renderer.
pub struct Show {
    scene: Scene,
    arm: Arm,
    lab: lab::Lab,
    letter_nodes: Vec<NodeId>,
    /// Dust in the air, lit by the eye in the dark lab.
    dust: NodeId,
    /// The sparks of the dropped letter.
    sparks: NodeId,
    pub story: Story,
    renderer: Renderer,
    read_back: bool,
}

impl std::fmt::Debug for Show {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Show").finish_non_exhaustive()
    }
}

impl Show {
    pub fn new() -> Self {
        let mut scene = Scene::new();
        let lab = lab::build(&mut scene, FONT);
        let arm = Arm::build(&mut scene, &Palette::new(), FONT);
        let story = Story::new(FONT, WORD);
        let lacquer = Material::standard(0x111113).roughness(0.2).clearcoat(1.0);
        let letter_nodes = story
            .letters
            .iter()
            .map(|l| scene.add(Node::mesh(l.geometry.clone(), lacquer.clone())))
            .collect();
        let dust = scene.add(Node::particles(Vec::new(), Blending::Additive));
        let sparks = scene.add(Node::particles(Vec::new(), Blending::Additive));
        Self {
            scene,
            arm,
            lab,
            letter_nodes,
            dust,
            sparks,
            story,
            renderer: Renderer::new(WIDTH as u32, HEIGHT as u32)
                .gpu()
                .tone_mapping(ToneMapping::Neutral)
                .exposure(0.92)
                .bloom(Bloom {
                    strength: 0.4,
                    radius: 0.45,
                    threshold: 1.2,
                }),
            read_back: false,
        }
    }

    /// Length of the film in seconds.
    pub fn duration(&self) -> f32 {
        self.story.beats.end + 0.6
    }

    /// Renders the 3D with the CPU rasterizer instead of the GPU.
    pub fn on_cpu(mut self) -> Self {
        self.renderer = self.renderer.cpu();
        self
    }

    /// Reads the 3D back from the GPU and gives Skia pixels instead of the GPU texture.
    /// Use it to compare and to measure.
    pub fn read_back(mut self) -> Self {
        self.read_back = true;
        self
    }

    /// The 3D image of the frame at `t`. It is a GPU texture when the backend shares the
    /// GPU, and pixels otherwise.
    pub fn image(&self, t: f32) -> fframes::media::ImageData<'static> {
        if self.motion_blur(t) {
            // Three moments in the open shutter blur the spin and the tumble.
            let moment = |offset: f32| self.scene_at(t + offset * SHUTTER / FPS as f32);
            if self.read_back {
                return self
                    .renderer
                    .render_motion_blur_pixels(3, moment)
                    .into_image();
            }
            return self.renderer.render_motion_blur(3, moment);
        }
        let (scene, camera) = self.scene_at(t);
        if self.read_back {
            return self.renderer.render_pixels(&scene, &camera).into_image();
        }
        self.renderer.render(&scene, &camera)
    }

    /// The fast part: the spin, the throw and the tumble of the letter.
    fn motion_blur(&self, t: f32) -> bool {
        (self.show_off()..self.reaction()).contains(&t)
    }

    /// The time when the robot starts to show off with the "a".
    fn show_off(&self) -> f32 {
        self.placed_time(2) + 0.45
    }

    /// The time when the robot looks into the camera after the throw.
    fn reaction(&self) -> f32 {
        self.story.beats.drop + 1.25
    }

    /// The light level of the lab at `t`: off, a flicker, then on.
    fn lights(&self, t: f32) -> f32 {
        let on = self.story.beats.lights;
        if t < on {
            return 0.0;
        }
        let x = t - on;
        if x < 0.35 {
            // The tubes go on and off at these times.
            let flick = [0.0f32, 0.06, 0.12, 0.2, 0.27];
            let lit = flick.iter().filter(|&&f| x >= f).count() % 2 == 1;
            if lit { 0.75 } else { 0.08 }
        } else {
            smoothstep(0.35, 0.6, x) * 0.25 + 0.75
        }
    }

    /// The mean of `point` over the last 0.2 s, so that the cameras follow smoothly.
    fn lagged(&self, t: f32, point: impl Fn(&Joints) -> Vec3) -> Vec3 {
        (0..5)
            .map(|k| point(&self.story.track.sample((t - k as f32 * 0.05).max(0.0)).q))
            .sum::<Vec3>()
            / 5.0
    }

    fn lagged_tip(&self, t: f32) -> Vec3 {
        self.lagged(t, |q| arm::tip(q).w_axis.truncate())
    }

    fn lagged_eye(&self, t: f32) -> Vec3 {
        self.lagged(t, arm::eye)
    }

    /// The time at which letter `i` goes into its place for the last time.
    fn placed_time(&self, i: usize) -> f32 {
        let letter = &self.story.letters[i];
        letter
            .phases
            .iter()
            .rev()
            .find(|(_, p)| matches!(p, Phase::Still(m) if *m == letter.slot))
            .map_or(0.0, |(t, _)| *t)
    }

    /// The cut list: one camera for each part of the story.
    fn camera(&self, t: f32) -> Camera {
        let b = &self.story.beats;
        let lens = Camera::perspective(32.0);
        let r_start = self.placed_time(2) - 1.2;
        let (show_off, reaction) = (self.show_off(), self.reaction());
        let last = self.placed_time(6);
        if t < b.work {
            // Hello: a low camera at the front right. The robot looks into this lens.
            let x = progress(t, 0.0, b.work);
            let eye = self.lagged_eye(t);
            lens.fov(30.0)
                .position(story::HELLO_CAMERA + vec3(0.25 * (1.0 - x), 0.05 * x, 0.35 * (1.0 - x)))
                .look_at(eye.lerp(vec3(0.0, 0.9, 0.6), 0.35))
                .focus_on(eye, 5.0)
        } else if t < r_start {
            // Work: a slow orbit that follows the gripper.
            let x = progress(t, b.work, r_start);
            let tip = self.lagged_tip(t);
            lens.orbit(
                vec3(0.0, 0.75, 0.2).lerp(tip, 0.25),
                5.4 - 0.4 * x,
                -42.0 + 24.0 * x,
                13.0,
            )
            .focus_on(tip, 8.0)
        } else if t < show_off {
            // Low at the word when the r arrives.
            let target = vec3(-0.35, 0.33, WORD_Z);
            lens.fov(30.0)
                .position(vec3(1.35, 0.42, 3.15))
                .look_at(target)
                .focus_on(target, 12.0)
        } else if t < reaction {
            // Wide for the swing and the throw. Each hit of the letter shakes the camera.
            let x = progress(t, show_off, reaction);
            let camera = lens
                .fov(36.0)
                .position(vec3(3.7 - 0.3 * x, 1.75, 4.4 - 0.3 * x))
                .look_at(vec3(-0.25, 0.75, 0.25))
                .focus(5.6, 5.0);
            let hits = self.story.falls.iter().flat_map(|f| &f.impacts);
            effects::shake(camera, t, hits.map(|&(at, speed)| (at, 0.35 * speed)))
        } else if t < b.recover {
            // Close on the eye. The robot looks into the lens.
            let eye = self.lagged_eye(t);
            lens.fov(28.0)
                .position(story::GUILTY_CAMERA)
                .look_at(eye.lerp(vec3(-0.2, 0.6, 0.6), 0.15))
                .focus_on(eye, 14.0)
        } else if t < b.finish {
            // Low near the fallen letter while the robot moves over carefully.
            let tip = self.lagged_tip(t);
            let letter = self.story.letters[self.story.dropped]
                .world(t, &self.story.track, &self.story.falls)
                .w_axis
                .truncate();
            let target = letter.lerp(tip, 0.4);
            lens.fov(34.0)
                .position(vec3(2.9, 0.75, 2.9))
                .look_at(target + vec3(0.0, 0.1, 0.0))
                .focus_on(target, 9.0)
        } else if t < last + 0.3 {
            // High and wide for the fast work.
            let x = progress(t, b.finish, last + 0.3);
            lens.orbit(vec3(0.0, 0.55, 0.35), 6.4, 28.0 - 18.0 * x, 24.0 - 4.0 * x)
                .focus(6.4, 4.0)
        } else {
            // The word, with the robot behind it as it bows.
            let x = smoothstep(last + 0.3, b.end, t);
            lens.fov(30.0)
                .position(story::FINAL_CAMERA + vec3(0.0, -0.05 * x, -0.5 * x))
                .look_at(vec3(0.0, 0.92, 0.55))
                .focus(story::FINAL_CAMERA.z - WORD_Z - 0.5 * x, 6.0)
        }
    }

    /// Dust in the air around the robot. Only the dust in the cone of the eye light shows,
    /// as in a beam of sunlight: `beam` is how much the beam shows.
    fn dust(&self, t: f32, q: &Joints, beam: f32) -> Vec<Particle> {
        if beam <= 0.0 {
            return Vec::new();
        }
        let eye = arm::eye(q);
        let axis = arm::flange(q).z_axis.truncate();
        let color = Rgb::hex(0xffd6a8);
        (0..4000u32)
            .filter_map(|i| {
                let r = |k: u32| hash(i * 4 + k);
                let home = vec3(-1.4 + 3.4 * r(0), 0.05 + 2.4 * r(1), -0.8 + 3.4 * r(2));
                // The dust drifts slowly and sinks.
                let drift = vec3(
                    noise(i, t * 0.25),
                    noise(i + 7919, t * 0.2),
                    noise(i + 15_485, t * 0.25),
                );
                let p = home + drift * 0.08 - Vec3::Y * 0.01 * t;
                let to = p - eye;
                let distance = to.length();
                let cone = smoothstep(deg(17.0).cos(), deg(9.0).cos(), to.dot(axis) / distance);
                let light = beam * cone / (1.0 + distance * distance);
                (light > 0.01).then(|| {
                    // A few pixels wide: the depth of field gathers on a spiral, and it
                    // breaks a dot of one bright pixel into a pattern of dots.
                    Particle::new(p, 0.006 + 0.004 * r(3), color * (3.0 * light))
                        .opacity(light.min(1.0))
                })
            })
            .collect()
    }

    /// The sparks of the dropped letter where it hits the floor.
    fn sparks(&self, t: f32) -> Vec<Particle> {
        let letter = &self.story.letters[self.story.dropped];
        self.story
            .falls
            .iter()
            .flat_map(|fall| &fall.impacts)
            .enumerate()
            .filter(|&(_, &(_, speed))| speed > 0.8)
            .flat_map(|(i, &(at, speed))| {
                let center = letter
                    .world(at, &self.story.track, &self.story.falls)
                    .w_axis;
                let hit = vec3(center.x, 0.01, center.z);
                let count = (speed * 12.0) as u32;
                effects::sparks(hit, at, speed * 0.9, count, i as u32, t, 1.5 / FPS as f32)
            })
            .collect()
    }

    /// True while the gripper holds a letter.
    fn carrying(&self, t: f32) -> bool {
        self.story
            .letters
            .iter()
            .any(|l| matches!(l.phase(t), Phase::Carried(_)))
    }

    /// The posed scene and the camera at `t`.
    pub fn scene_at(&self, t: f32) -> (Scene, Camera) {
        let mut scene = self.scene.clone();
        lab::set_lights(&mut scene, &self.lab, self.lights(t));
        let state = self.story.track.sample(t);
        let wake = self.story.beats.wake;
        let booting = smoothstep(wake, wake + 0.5, t);
        // The eye flickers when it comes on.
        let flicker = if (wake + 0.5..wake + 0.8).contains(&t)
            && hash(((t - wake) * 40.0) as u32 + 5) > 0.5
        {
            0.3
        } else {
            1.0
        };
        let beam = (1.0 - self.lights(t)).clamp(0.0, 1.0);
        let face = Expression {
            ring: 0.8
                + booting * (2.2 + 0.8 * (t * 2.0).sin().max(0.0))
                + 3.0 * smoothstep(self.story.beats.tada, self.story.beats.tada + 0.3, t),
            status: if self.carrying(t) { 7.0 } else { 0.5 * booting },
            eye: 7.0 * smoothstep(wake + 0.5, wake + 0.8, t) * flicker,
            lid: self.story.lid(t),
            beam,
        };
        self.arm.apply(&mut scene, &state.q, state.gap, &face);
        // The eye glows up to 7.
        let dust = self.dust(t, &state.q, beam * face.eye / 7.0 * face.lid);
        if let Object::Particles(p) = &mut scene[self.dust].object {
            p.items = dust;
        }
        if let Object::Particles(p) = &mut scene[self.sparks].object {
            p.items = self.sparks(t);
        }
        for (letter, node) in self.story.letters.iter().zip(&self.letter_nodes) {
            let (_, rotation, position) = letter
                .world(t, &self.story.track, &self.story.falls)
                .to_scale_rotation_translation();
            scene[*node].position = position;
            scene[*node].rotation = rotation;
        }
        (scene, self.camera(t))
    }

    /// A bracket on the thrown letter while it lies on the floor, pinned to it with
    /// [`Camera::project`] and sized with [`Camera::pixel_size`].
    fn marker(&self, t: f32, camera: &Camera) -> Svgr<'static> {
        let letter = &self.story.letters[self.story.dropped];
        let lost = match letter.phase(t) {
            Phase::Falling(_) => true,
            Phase::Still(m) => *m != letter.slot,
            Phase::Carried(_) => false,
        };
        let shown = smoothstep(self.story.beats.drop + 0.3, self.story.beats.drop + 0.5, t);
        if !lost || shown <= 0.0 {
            return Svgr::empty();
        }
        let world = letter.world(t, &self.story.track, &self.story.falls);
        let center = world.transform_point3(letter.bounds.center());
        let Some(at) = camera.project(center, WIDTH as f32, HEIGHT as f32) else {
            return Svgr::empty();
        };
        let size = camera.pixel_size(center, 0.42, HEIGHT as f32);
        let (x0, y0) = (at.x - size * 0.5, at.y - size * 0.5);
        let (x1, y1) = (x0 + size, y0 + size);
        let corner = (size * 0.22).max(8.0);
        let corners = format!(
            "M{x0} {} V{y0} H{} M{} {y0} H{x1} V{} M{x1} {} V{y1} H{} M{} {y1} H{x0} V{}",
            y0 + corner,
            x0 + corner,
            x1 - corner,
            y0 + corner,
            y1 - corner,
            x1 - corner,
            x0 + corner,
            y1 - corner,
        );
        let slot = letter.slot.transform_point3(letter.bounds.center());
        let away = format!("{:.2} M FROM SLOT", center.distance(slot));
        let name = WORD.chars().nth(self.story.dropped).unwrap_or('?');
        let label = format!("PIECE {:02}  \"{name}\"", self.story.dropped + 1);
        svgr!(
            <g opacity={shown} font-family={MONO} fill={ORANGE}>
                <path d={corners} fill="none" stroke={ORANGE} stroke-width="2.5" />
                <text x={x1 + 12.0} y={y0 + 14.0} font-weight="600" font-size="15" letter-spacing="2">{label}</text>
                <text x={x1 + 12.0} y={y0 + 34.0} font-weight="400" font-size="13" letter-spacing="1.5">{away}</text>
            </g>
        )
    }

    /// The console, the counter, the caption and the fades.
    fn overlay(&self, t: f32) -> Svgr<'static> {
        let placed = self
            .story
            .letters
            .iter()
            .filter(|l| matches!(l.phase(t), Phase::Still(m) if *m == l.slot))
            .count();
        let b = &self.story.beats;
        // The console is dark ink. It shows once the lab is lit.
        let ui =
            smoothstep(b.lights, b.lights + 0.4, t) * (1.0 - smoothstep(b.end - 0.6, b.end, t));
        // The console shows the last five lines. They type out, and the newest is at the bottom.
        let shown: Vec<_> = self.story.log.iter().filter(|l| l.t <= t).collect();
        let tail = &shown[shown.len().saturating_sub(5)..];
        let lines: Vec<Svgr> = tail
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let chars = (((t - line.t) * 45.0) as usize).min(line.text.len());
                let text: String = line.text.chars().take(chars).collect();
                let y = 1000.0 - (tail.len() - 1 - i) as f32 * 24.0;
                let fade = 0.35 + 0.65 * (i as f32 + 1.0) / tail.len() as f32;
                let color = if line.alert { ORANGE } else { INK };
                let cursor = if i == tail.len() - 1 && (t * 2.5).fract() < 0.5 { "_" } else { "" };
                svgr!(<text x="96" y={y} fill={color} opacity={fade}>{format!("> {text}{cursor}")}</text>)
            })
            .collect();
        let caption = smoothstep(b.tada + 0.3, b.tada + 0.8, t);
        let d = self.duration();
        let black = (1.0 - smoothstep(0.0, 0.4, t)).max(smoothstep(d - 0.5, d, t));
        svgr!(
            <g>
                <g opacity={ui} font-family={MONO} font-weight="600" font-size="14" letter-spacing="3" fill={INK}>
                    <text x="96" y="100">"OD-ARM 7.2"</text>
                    <text x="96" y="122" opacity="0.5">"LAB 3  /  CELL R-07"</text>
                    <text x="1824" y="100" text-anchor="end">{format!("{placed:02} / {:02}", self.story.letters.len())}</text>
                    <text x="1824" y="122" text-anchor="end" opacity="0.5">"PIECES IN PLACE"</text>
                    <g font-weight="400" font-size="15" letter-spacing="1.5">{lines}</g>
                </g>
                <g opacity={caption} font-family={MONO} fill={INK}>
                    <rect x="938" y="968" width="44" height="2" fill={ORANGE} />
                    <text x="960" y="1004" text-anchor="middle" font-weight="400" font-size="13" letter-spacing="6" opacity="0.6">"BUILT WITH FFRAMES-3D"</text>
                </g>
                <rect x="0" y="0" width="1920" height="1080" fill="#000000" opacity={black} />
            </g>
        )
    }
}

impl Default for Show {
    fn default() -> Self {
        Self::new()
    }
}

/// The sound file and the level in dB of an event.
fn effect(sound: Sound) -> (&'static str, f32) {
    match sound {
        Sound::LightsOn => ("lights.wav", -4.0),
        Sound::Boot => ("boot.wav", -16.0),
        Sound::Blink => ("blink.wav", -16.0),
        Sound::Curious => ("curious.wav", -15.0),
        Sound::Happy => ("happy.wav", -16.0),
        Sound::Grip => ("grip.wav", -10.0),
        Sound::Place => ("place.wav", -5.0),
        Sound::Whoosh => ("whoosh.wav", -16.0),
        Sound::Scratch => ("scratch.wav", -4.0),
        Sound::Clatter(speed) => ("clatter.wav", -14.0 + 4.0 * speed.min(4.0)),
        Sound::UhOh => ("uhoh.wav", -13.0),
        Sound::Sigh => ("sigh.wav", -14.0),
        Sound::Pat => ("pat.wav", -9.0),
        Sound::Proud => ("proud.wav", -15.0),
        Sound::TaDa => ("tada.wav", -14.0),
    }
}

impl Video for Show {
    const FPS: usize = FPS;
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(Show::duration(self))
    }

    fn audio(&self) -> AudioMap<'_> {
        use AudioTimestamp::{Eof, Second};
        let mut tracks = vec![
            AudioTrack::new("music.wav", Second(0.0)..Eof).gain_db(-4.0),
            AudioTrack::new("room.wav", Second(0.0)..Eof)
                .gain_db(-18.0)
                .fade_in(0.5),
            AudioTrack::new("servos.wav", Second(0.0)..Eof).gain_db(-11.0),
        ];
        for &(t, sound) in &self.story.sounds {
            let (file, gain) = effect(sound);
            tracks.push(AudioTrack::new(file, Second(t.max(0.0))..Eof).gain_db(gain));
        }
        AudioMap::from(tracks)
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let image = self.image(t);
        let marker = self.marker(t, &self.camera(t));
        let overlay = self.overlay(t);
        svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1920 1080" width={WIDTH} height={HEIGHT}>
                <image href={image.href()} x="0" y="0" width="1920" height="1080" />
                {marker}
                {overlay}
            </svg>
        )
    }
}
