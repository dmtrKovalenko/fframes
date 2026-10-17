//! The performance of the arm: keys with anticipation, overshoot and idle motion. It also
//! holds the timelines of the letters, the tumble of the dropped letter and the events.

use fframes_3d::math::smoothstep;
use fframes_3d::*;

use crate::arm::{self, Joints, OPEN, frame, looking, solve, tool_down};

/// The easing into a key.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ease {
    /// Slow start and slow stop (quintic).
    Smooth,
    /// Fast start and gentle stop.
    Out,
    /// Gentle start and fast stop.
    In,
    /// Goes past the key by `s` and comes back.
    OutBack(f32),
    /// Moves back by `s` before it goes (anticipation).
    InBack(f32),
}

impl Ease {
    pub fn apply(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Ease::Smooth => x * x * x * (x * (x * 6.0 - 15.0) + 10.0),
            Ease::Out => 1.0 - (1.0 - x).powi(3),
            Ease::In => x * x * x,
            Ease::OutBack(s) => 1.0 + (s + 1.0) * (x - 1.0).powi(3) + s * (x - 1.0).powi(2),
            Ease::InBack(s) => (s + 1.0) * x * x * x - s * x * x,
        }
    }
}

/// A key of the arm animation.
#[derive(Debug, Clone, Copy)]
pub struct Key {
    pub t: f32,
    pub q: Joints,
    /// Half the finger gap.
    pub gap: f32,
    pub ease: Ease,
    /// Amplitude of the idle motion.
    pub alive: f32,
}

/// The arm at one moment.
#[derive(Debug, Clone, Copy)]
pub struct ArmState {
    pub q: Joints,
    pub gap: f32,
}

/// The keys of the arm, in time order.
#[derive(Debug, Clone, Default)]
pub struct Track {
    pub keys: Vec<Key>,
}

impl Track {
    pub fn sample(&self, t: f32) -> ArmState {
        let k = self.keys.partition_point(|k| k.t <= t);
        let (q, gap, alive) = if k == 0 {
            let f = &self.keys[0];
            (f.q, f.gap, f.alive)
        } else if k >= self.keys.len() {
            let l = &self.keys[self.keys.len() - 1];
            (l.q, l.gap, l.alive)
        } else {
            let (a, b) = (&self.keys[k - 1], &self.keys[k]);
            let s = (t - a.t) / (b.t - a.t).max(1e-6);
            let x = b.ease.apply(s);
            let linear = s.clamp(0.0, 1.0);
            (
                a.q.lerp(&b.q, x),
                a.gap + (b.gap - a.gap) * x,
                a.alive + (b.alive - a.alive) * linear,
            )
        };
        // Idle motion: each joint sways slowly with its own frequency and phase.
        let idle = [
            0.035 * (t * 1.3).sin(),
            0.03 * (t * 1.7 + 1.0).sin(),
            0.04 * (t * 2.1 + 2.0).sin(),
            0.05 * (t * 0.9 + 3.0).sin(),
            0.07 * (t * 1.1 + 0.5).sin(),
            0.04 * (t * 0.7 + 1.5).sin(),
        ];
        ArmState {
            q: q.add(idle.map(|v| v * alive)),
            gap,
        }
    }
}

/// Where a letter is during one part of its timeline.
#[derive(Debug, Clone)]
pub enum Phase {
    /// At rest with this world transform.
    Still(Mat4),
    /// Held by the gripper with this offset from the grip point.
    Carried(Mat4),
    /// Falls along the tumble with this index in [`Story::falls`].
    Falling(usize),
}

/// A precomputed tumble of a dropped letter.
#[derive(Debug, Clone)]
pub struct Fall {
    pub start: f32,
    /// The transforms at [`Fall::RATE`] steps per second.
    pub frames: Vec<Mat4>,
    /// The time and the speed in m/s of each hit on the floor.
    pub impacts: Vec<(f32, f32)>,
}

impl Fall {
    /// Simulation steps per second.
    pub const RATE: f32 = 480.0;

    /// The transform at `t`, interpolated between the simulation steps.
    pub fn at(&self, t: f32) -> Mat4 {
        let x = ((t - self.start) * Self::RATE).max(0.0);
        let i = (x as usize).min(self.frames.len() - 1);
        let j = (i + 1).min(self.frames.len() - 1);
        let f = (x - i as f32).clamp(0.0, 1.0);
        let (sa, ra, ta) = self.frames[i].to_scale_rotation_translation();
        let (_, rb, tb) = self.frames[j].to_scale_rotation_translation();
        Mat4::from_scale_rotation_translation(sa, ra.slerp(rb, f), ta.lerp(tb, f))
    }

    /// The time at which the tumble stops.
    pub fn end(&self) -> f32 {
        self.start + self.frames.len() as f32 / Self::RATE
    }
}

/// A letter piece and its timeline.
#[derive(Debug, Clone)]
pub struct Letter {
    pub geometry: Geometry,
    /// Bounds in glyph space.
    pub bounds: Aabb,
    /// The transform of the letter in the rack.
    pub rack: Mat4,
    /// The transform of the letter in the word.
    pub slot: Mat4,
    /// The grip point on top of the letter, in glyph space.
    pub top: Vec3,
    /// Center and half width of the rightmost stem, in glyph space.
    pub stem: (Vec3, f32),
    /// The start time of each phase, in time order.
    pub phases: Vec<(f32, Phase)>,
}

impl Letter {
    /// The phase of the letter at `t`.
    pub fn phase(&self, t: f32) -> &Phase {
        let i = self
            .phases
            .partition_point(|(s, _)| *s <= t)
            .saturating_sub(1);
        &self.phases[i].1
    }

    /// The world transform of the letter at `t`.
    pub fn world(&self, t: f32, track: &Track, falls: &[Fall]) -> Mat4 {
        match self.phase(t) {
            Phase::Still(m) => *m,
            Phase::Carried(rel) => arm::tip(&track.sample(t).q) * *rel,
            Phase::Falling(f) => falls[*f].at(t),
        }
    }
}

/// The sound events.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sound {
    LightsOn,
    Boot,
    Blink,
    Curious,
    Happy,
    Grip,
    Place,
    Whoosh,
    Scratch,
    /// A hit on the floor at this speed in m/s.
    Clatter(f32),
    UhOh,
    Sigh,
    Pat,
    Proud,
    TaDa,
}

/// A line of the robot console.
#[derive(Debug, Clone)]
pub struct LogLine {
    pub t: f32,
    pub text: &'static str,
    /// The overlay shows the line in orange.
    pub alert: bool,
}

/// The story beats. The cameras, the music and the overlay follow them.
#[derive(Debug, Clone, Copy, Default)]
pub struct Beats {
    pub lights: f32,
    pub wake: f32,
    pub work: f32,
    pub drop: f32,
    pub recover: f32,
    pub finish: f32,
    pub tada: f32,
    pub end: f32,
}

/// The whole performance, built once.
pub struct Story {
    pub track: Track,
    pub letters: Vec<Letter>,
    pub falls: Vec<Fall>,
    /// The sound events, in time order.
    pub sounds: Vec<(f32, Sound)>,
    pub log: Vec<LogLine>,
    pub blinks: Vec<f32>,
    pub beats: Beats,
    /// Index of the letter that slips.
    pub dropped: usize,
}

/// The camera the robot looks into after the drop.
pub const GUILTY_CAMERA: Vec3 = Vec3::new(0.55, 1.32, 3.25);
/// The camera the robot bows to at the end.
pub const FINAL_CAMERA: Vec3 = Vec3::new(0.0, 1.25, 7.2);
/// The camera the robot looks into when it wakes up.
pub const HELLO_CAMERA: Vec3 = Vec3::new(3.2, 1.25, 4.9);

/// Adds the keys one after the other. The last key is the inverse kinematics hint for the
/// next key.
struct Director {
    track: Track,
    t: f32,
}

impl Director {
    fn last(&self) -> Key {
        *self.track.keys.last().expect("first key")
    }

    /// Adds a key `dt` seconds after the last key.
    fn key(&mut self, dt: f32, q: Joints, gap: f32, ease: Ease, alive: f32) {
        self.t += dt;
        self.track.keys.push(Key {
            t: self.t,
            q,
            gap,
            ease,
            alive,
        });
    }

    /// Adds a key that puts the grip point at `tip` with the tool frame `rotation`.
    fn pose(
        &mut self,
        dt: f32,
        tip: Vec3,
        rotation: Quat,
        gap: f32,
        ease: Ease,
        alive: f32,
    ) -> Joints {
        let q = solve(tip, rotation, &self.last().q);
        self.key(dt, q, gap, ease, alive);
        q
    }

    /// Adds a key with the eye at `at`, looking at `target`.
    fn look(
        &mut self,
        dt: f32,
        at: Vec3,
        target: Vec3,
        roll: f32,
        ease: Ease,
        alive: f32,
    ) -> Joints {
        let gap = self.last().gap;
        self.pose(dt, at, looking(target - at, roll), gap, ease, alive)
    }

    /// Keeps the last pose for `dt` seconds.
    fn hold(&mut self, dt: f32, alive: f32) {
        let k = self.last();
        self.key(dt, k.q, k.gap, Ease::Smooth, alive);
    }

    /// Opens or closes the fingers to `gap`.
    fn gap(&mut self, dt: f32, gap: f32) {
        let k = self.last();
        self.key(dt, k.q, gap, Ease::Out, 0.0);
    }

    /// Adds `offsets` to the joint angles of the last key.
    fn joints(&mut self, dt: f32, offsets: [f32; 6], ease: Ease, alive: f32) {
        let k = self.last();
        self.key(dt, k.q.add(offsets), k.gap, ease, alive);
    }
}

/// The inside spans of a glyph at height `y`, by the even-odd rule.
fn spans(shapes: &[Shape], y: f32) -> Vec<(f32, f32)> {
    let mut xs = Vec::new();
    for shape in shapes {
        for contour in std::iter::once(&shape.outer).chain(&shape.holes) {
            for i in 0..contour.len() {
                let (a, b) = (contour[i], contour[(i + 1) % contour.len()]);
                if (a.y > y) != (b.y > y) {
                    xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
        }
    }
    xs.sort_by(f32::total_cmp);
    xs.as_chunks::<2>().0.iter().map(|c| (c[0], c[1])).collect()
}

/// Simulates the tumble of a rigid letter: gravity and bounces that lose energy. Then the
/// letter settles flat on its back, as a thin plate does.
fn simulate_fall(start: f32, m0: Mat4, v0: Vec3, w0: Vec3, bounds: &Aabb) -> Fall {
    let corners = bounds.corners();
    let (_, mut rot, mut pos) = m0.to_scale_rotation_translation();
    let (mut v, mut w) = (v0, w0);
    let dt = 1.0 / Fall::RATE;
    let mut frames = Vec::new();
    let mut impacts = Vec::new();
    let mut t = 0.0;
    let mut settling: Option<(f32, Quat, Vec3)> = None;
    loop {
        frames.push(Mat4::from_rotation_translation(rot, pos));
        if let Some((since, from_rot, from_pos)) = settling {
            let x = ((t - since) / 0.45).min(1.0);
            // The letter settles with the front face up. The up axis of the glyph keeps the
            // horizontal direction of the fall.
            let up = rot * Vec3::Y;
            let heading = vec3(up.x, 0.0, up.z).normalize_or(Vec3::NEG_Z);
            let target_rot = Quat::from_mat3(&Mat3::from_cols(
                heading.cross(Vec3::Y).normalize(),
                heading,
                Vec3::Y,
            ));
            let e = 1.0 - (1.0 - x).powi(3);
            rot = from_rot.slerp(target_rot, e);
            let slide = from_pos + v * 0.12 * e;
            pos = vec3(slide.x, 0.0, slide.z);
            // The lowest corner stays on the floor.
            let lowest = corners
                .iter()
                .map(|c| (rot * *c).y)
                .fold(f32::INFINITY, f32::min);
            pos.y = -lowest + (1.0 - e) * 0.03 * (x * 20.0).sin().abs();
            if x >= 1.0 {
                frames.push(Mat4::from_rotation_translation(rot, pos));
                break;
            }
        } else {
            v.y -= 9.81 * dt;
            pos += v * dt;
            rot = (Quat::from_scaled_axis(w * dt) * rot).normalize();
            let lowest = corners
                .iter()
                .map(|c| (rot * *c + pos).y)
                .fold(f32::INFINITY, f32::min);
            if lowest < 0.0 && v.y < 0.0 {
                pos.y -= lowest;
                impacts.push((start + t, -v.y));
                v.y = -v.y * 0.32;
                v.x *= 0.55;
                v.z *= 0.55;
                // The corner that hits the floor changes the spin.
                w = w * 0.45 + vec3(v.z, 0.0, -v.x) * 3.0;
                if impacts.len() >= 2 || v.y < 0.7 {
                    settling = Some((t, rot, pos));
                }
            }
        }
        t += dt;
        if t > 4.0 {
            break;
        }
    }
    Fall {
        start,
        frames,
        impacts,
    }
}

impl Story {
    /// Builds the performance for `word`, with letters cut from `font`.
    pub fn new(font: &[u8], word: &str) -> Story {
        const SIZE: f32 = 0.68;
        const DEPTH: f32 = 0.1;
        const TRACKING: f32 = 0.035;
        let glyphs = Geometry::text_glyphs(font, word, SIZE, DEPTH, 0.012)
            .expect("the font has the letters");
        let shapes =
            Geometry::text_shapes(font, word, SIZE, 0.0).expect("the font has the letters");
        let count = glyphs.len();
        let width =
            glyphs.last().map_or(0.0, |g| g.origin.x + g.advance) + TRACKING * (count - 1) as f32;
        // The letter at each rack position, from left to right.
        let rack_order = [4usize, 1, 6, 0, 3, 5, 2];
        let mut letters: Vec<Letter> = glyphs
            .into_iter()
            .zip(shapes)
            .enumerate()
            .map(|(i, (glyph, outline))| {
                let bounds = glyph.geometry.bounds();
                let top_y = bounds.max.y;
                let near: Vec<f32> = glyph
                    .geometry
                    .positions
                    .iter()
                    .filter(|p| p.y > top_y - 0.03)
                    .map(|p| p.x)
                    .collect();
                let grip_x = near.iter().sum::<f32>() / near.len().max(1) as f32;
                let stem_y = 0.32 * (bounds.max.y - bounds.min.y) + bounds.min.y;
                let stem = spans(&outline.shapes, stem_y)
                    .last()
                    .map_or((vec3(bounds.center().x, stem_y, 0.0), 0.05), |&(a, b)| {
                        (vec3(f32::midpoint(a, b), stem_y, 0.0), (b - a) * 0.5)
                    });
                let slot_index = rack_order.iter().position(|&r| r == i).unwrap_or(i) as f32;
                let rack_x = -1.56 + slot_index * 0.52 - bounds.center().x;
                let rack = Mat4::from_translation(vec3(rack_x, 0.02, crate::RACK_Z));
                let slot = Mat4::from_translation(vec3(
                    glyph.origin.x + i as f32 * TRACKING - width * 0.5,
                    0.0,
                    crate::WORD_Z,
                ));
                Letter {
                    geometry: glyph.geometry,
                    bounds,
                    rack,
                    slot,
                    top: vec3(grip_x, top_y - 0.035, 0.0),
                    stem,
                    phases: vec![(0.0, Phase::Still(rack))],
                }
            })
            .collect();

        let mut sounds: Vec<(f32, Sound)> = Vec::new();
        let mut log: Vec<LogLine> = Vec::new();
        let mut blinks = Vec::new();
        let mut falls = Vec::new();
        let mut beats = Beats::default();
        let say = |log: &mut Vec<LogLine>, t: f32, text: &'static str, alert: bool| {
            log.push(LogLine { t, text, alert });
        };

        // ---------------------------------------------------------------- Asleep
        let asleep_tip = vec3(0.05, 0.24, 0.95);
        let asleep = solve(
            asleep_tip,
            looking(vec3(0.05, -1.0, 0.45), 0.25),
            &Joints {
                q: [0.0, 0.4, 1.0, 0.0, 0.8, 0.0],
            },
        );
        let mut d = Director {
            track: Track {
                keys: vec![Key {
                    t: 0.0,
                    q: asleep,
                    gap: 0.03,
                    ease: Ease::Smooth,
                    alive: 0.15,
                }],
            },
            t: 0.0,
        };
        // The robot wakes up in the dark lab. Its eye comes on and lights the floor.
        beats.wake = 0.8;
        sounds.push((0.75, Sound::Boot));
        say(&mut log, 0.8, "BOOT  OD-ARM 7.2 ...... OK", false);
        d.hold(2.0, 0.15);

        // ---------------------------------------------------------------- Waking up
        // A slow, tired lift with a sag halfway, a big stretch, then curiosity.
        d.look(
            0.9,
            vec3(0.1, 0.62, 0.95),
            vec3(0.2, 0.0, 2.2),
            0.15,
            Ease::InBack(0.4),
            0.3,
        );
        d.joints(0.35, [0.0, 0.08, 0.1, 0.0, 0.25, 0.0], Ease::Smooth, 0.2);
        blinks.push(d.t + 0.1);
        sounds.push((d.t + 0.1, Sound::Blink));
        d.look(
            0.75,
            vec3(0.0, 2.15, 0.55),
            vec3(0.0, 3.5, 1.4),
            0.0,
            Ease::OutBack(1.6),
            0.0,
        );
        // The lab lights flicker on when it reaches up.
        beats.lights = d.t - 0.2;
        sounds.push((beats.lights, Sound::LightsOn));
        d.gap(0.2, OPEN);
        d.look(
            0.55,
            vec3(0.25, 1.35, 1.05),
            HELLO_CAMERA,
            0.35,
            Ease::OutBack(1.2),
            0.5,
        );
        sounds.push((d.t - 0.2, Sound::Curious));
        say(&mut log, d.t - 0.2, "VISION ONLINE  FACE DETECTED", false);
        d.look(
            0.25,
            vec3(0.25, 1.35, 1.05),
            HELLO_CAMERA,
            -0.25,
            Ease::Smooth,
            0.4,
        );
        blinks.push(d.t);
        sounds.push((d.t, Sound::Blink));
        d.hold(0.3, 0.6);
        // It looks at the rack, quickly at the camera, then happily back at the rack.
        let rack_center = vec3(0.0, 0.3, crate::RACK_Z);
        d.look(
            0.45,
            vec3(-0.2, 1.4, -0.35),
            rack_center,
            0.0,
            Ease::Smooth,
            0.3,
        );
        d.look(0.2, vec3(0.2, 1.4, 0.7), HELLO_CAMERA, 0.2, Ease::Out, 0.2);
        d.look(
            0.3,
            vec3(-0.15, 1.45, -0.4),
            rack_center,
            -0.1,
            Ease::OutBack(1.3),
            0.3,
        );
        sounds.push((d.t, Sound::Happy));
        say(&mut log, d.t, "TASK  SPELL \"fframes\"", false);
        d.hold(0.25, 0.5);
        beats.work = d.t;

        // ---------------------------------------------------------------- Work
        // The arm carries the letters this high above the rack and the word.
        let carry = 0.68;
        // The tool points down. The fingers clamp the thickness of the letter (world Z).
        let rot = tool_down(Vec3::Z);
        let pick_place = |d: &mut Director,
                          letters: &mut [Letter],
                          sounds: &mut Vec<(f32, Sound)>,
                          i: usize,
                          pace: f32,
                          flourish: bool| {
            let l = &letters[i];
            let grab = l.rack.transform_point3(l.top);
            let place = l.slot.transform_point3(l.top);
            d.pose(
                0.62 * pace,
                grab + Vec3::Y * carry,
                rot,
                OPEN,
                Ease::Smooth,
                0.0,
            );
            d.pose(0.24 * pace, grab, rot, OPEN, Ease::Smooth, 0.0);
            d.gap(0.12 * pace, 0.057);
            let t_grab = d.t - 0.04 * pace;
            sounds.push((t_grab - 0.05, Sound::Grip));
            let tip = arm::tip(&d.track.sample(t_grab).q);
            let rel = tip.inverse() * letters[i].rack;
            letters[i].phases.push((t_grab, Phase::Carried(rel)));
            d.pose(
                0.24 * pace,
                grab + Vec3::Y * carry,
                rot,
                0.057,
                Ease::OutBack(0.6),
                0.0,
            );
            sounds.push((d.t, Sound::Whoosh));
            d.pose(
                0.62 * pace,
                place + Vec3::Y * carry,
                rot,
                0.057,
                if flourish {
                    Ease::OutBack(1.0)
                } else {
                    Ease::Smooth
                },
                0.0,
            );
            d.pose(0.24 * pace, place, rot, 0.057, Ease::In, 0.0);
            let t_place = d.t;
            let slot = letters[i].slot;
            letters[i].phases.push((t_place, Phase::Still(slot)));
            sounds.push((t_place, Sound::Place));
            d.gap(0.1 * pace, OPEN);
            d.pose(
                0.22 * pace,
                place + Vec3::Y * 0.35,
                rot,
                OPEN,
                Ease::Out,
                0.2,
            );
            t_place
        };
        let t0 = pick_place(&mut d, &mut letters, &mut sounds, 0, 1.05, false);
        say(&mut log, t0, "PLACE  f ............. OK", false);
        let t1 = pick_place(&mut d, &mut letters, &mut sounds, 1, 0.8, false);
        say(&mut log, t1, "PLACE  f ............. OK", false);
        // It gets too confident: a small wiggle after the r.
        let t2 = pick_place(&mut d, &mut letters, &mut sounds, 2, 0.68, true);
        say(&mut log, t2, "PLACE  r ............. OK", false);
        d.joints(0.12, [0.0, 0.0, 0.0, 0.0, 0.0, 0.5], Ease::Out, 0.0);
        d.joints(0.12, [0.0, 0.0, 0.0, 0.0, 0.0, -1.0], Ease::Smooth, 0.0);
        d.joints(
            0.14,
            [0.0, 0.0, 0.0, 0.0, 0.0, 0.5],
            Ease::OutBack(1.0),
            0.0,
        );
        sounds.push((d.t, Sound::Happy));

        // ---------------------------------------------------------------- The swing
        let a = 3;
        let l = &letters[a];
        let grab = l.rack.transform_point3(l.top);
        d.pose(0.5, grab + Vec3::Y * carry, rot, OPEN, Ease::Smooth, 0.0);
        d.pose(0.2, grab, rot, OPEN, Ease::Smooth, 0.0);
        d.gap(0.1, 0.057);
        let t_grab = d.t - 0.03;
        sounds.push((t_grab - 0.05, Sound::Grip));
        let rel = arm::tip(&d.track.sample(t_grab).q).inverse() * letters[a].rack;
        letters[a].phases.push((t_grab, Phase::Carried(rel)));
        say(&mut log, d.t, "PLACE  a  WITH STYLE", false);
        // Up high, a small move back, then a big swing with a spin towards the word.
        let high = grab + vec3(0.0, 1.05, 0.25);
        d.pose(0.3, high, rot, 0.057, Ease::OutBack(0.8), 0.0);
        d.joints(0.25, [0.12, -0.1, -0.1, 0.0, 0.0, -0.6], Ease::Smooth, 0.0);
        let swing_from = d.t;
        let target_above = letters[a].slot.transform_point3(letters[a].top) + Vec3::Y * carry;
        let mut q = solve(target_above + vec3(0.25, 0.25, 0.0), rot, &d.last().q);
        q.q[5] += std::f32::consts::TAU * 1.5;
        d.key(0.95, q, 0.057, Ease::InBack(0.25), 0.0);
        sounds.push((swing_from + 0.15, Sound::Whoosh));
        // It opens the gripper a little too early.
        let t_drop = swing_from + 0.62;
        beats.drop = t_drop;
        let dt = 1.0 / 240.0;
        let at = |t: f32| arm::tip(&d.track.sample(t).q) * rel;
        let (m0, ma, mb) = (at(t_drop), at(t_drop - dt), at(t_drop + dt));
        let v0 = (mb.w_axis - ma.w_axis).truncate() / (2.0 * dt);
        let (_, ra, _) = ma.to_scale_rotation_translation();
        let (_, rb, _) = mb.to_scale_rotation_translation();
        let w0 = (rb * ra.inverse()).to_scaled_axis() / (2.0 * dt);
        // Aim the throw so that the letter lands in the reach of the arm and in view.
        let landing = vec3(1.3, 0.0, 0.4);
        let p0 = m0.w_axis.truncate();
        let vy = v0.y.clamp(-0.5, 1.5);
        let flight = (vy + (vy * vy + 2.0 * 9.81 * p0.y).sqrt()) / 9.81;
        let reach = (landing - p0) * 0.85 / flight;
        let v0 = vec3(reach.x, vy, reach.z);
        let fall = simulate_fall(t_drop, m0, v0, w0 * 0.6, &letters[a].bounds);
        let lying = *fall.frames.last().expect("frames");
        for &(t, speed) in &fall.impacts {
            sounds.push((t, Sound::Clatter(speed)));
        }
        let fall_end = fall.end();
        falls.push(fall);
        letters[a].phases.push((t_drop, Phase::Falling(0)));
        letters[a].phases.push((fall_end, Phase::Still(lying)));
        sounds.push((t_drop, Sound::Scratch));
        say(&mut log, t_drop + 0.05, "ERR 0x7A  GRIP LOST", true);
        sounds.push((t_drop - 0.05, Sound::Grip));

        // ---------------------------------------------------------------- After the drop
        d.gap(0.15, OPEN);
        d.hold(0.55, 0.0);
        let letter_at = lying.transform_point3(letters[a].bounds.center());
        let head = d.last();
        let head_pos = arm::tip(&head.q).w_axis.truncate();
        // A slow turn to the letter on the floor.
        d.look(
            0.8,
            head_pos + vec3(0.0, -0.1, 0.0),
            letter_at,
            0.2,
            Ease::Smooth,
            0.1,
        );
        sounds.push((d.t - 0.1, Sound::UhOh));
        say(&mut log, d.t, "STATUS  NOBODY SAW THAT", false);
        d.hold(0.5, 0.2);
        // Then it looks into the camera.
        let guilty = vec3(0.3, 1.3, 1.25);
        d.look(0.7, guilty, GUILTY_CAMERA, -0.25, Ease::Smooth, 0.0);
        blinks.push(d.t + 0.5);
        sounds.push((d.t + 0.5, Sound::Blink));
        d.hold(1.0, 0.3);
        // A quick look at the letter and back into the camera.
        d.look(0.16, guilty, letter_at, 0.1, Ease::Out, 0.0);
        d.hold(0.18, 0.0);
        d.look(0.16, guilty, GUILTY_CAMERA, -0.2, Ease::Out, 0.0);
        d.hold(0.35, 0.2);
        blinks.push(d.t - 0.1);
        say(&mut log, d.t, "SOMEBODY SAW THAT", true);
        // The sigh: the whole arm sags.
        d.joints(0.75, [0.0, 0.18, 0.28, 0.0, 0.45, 0.0], Ease::Smooth, 0.0);
        sounds.push((d.t - 0.6, Sound::Sigh));
        d.hold(0.35, 0.2);
        beats.recover = d.t;

        // ---------------------------------------------------------------- The careful recovery
        say(&mut log, d.t, "RETRY  a  (CAREFULLY)", false);
        let (stem_center, stem_half) = letters[a].stem;
        let stem_world = lying.transform_point3(stem_center);
        let (_, lying_rot, _) = lying.to_scale_rotation_translation();
        let grab_rot = lying_rot * frame(Vec3::X, Vec3::NEG_Z);
        // It moves over carefully and looks at the letter.
        d.look(
            0.75,
            stem_world + vec3(0.0, 0.55, 0.25),
            stem_world,
            0.3,
            Ease::Smooth,
            0.1,
        );
        d.hold(0.3, 0.2);
        d.pose(
            0.6,
            stem_world + Vec3::Y * 0.3,
            grab_rot,
            OPEN,
            Ease::Smooth,
            0.0,
        );
        d.pose(0.45, stem_world, grab_rot, OPEN, Ease::Smooth, 0.0);
        let stem_gap = stem_half + 0.007;
        d.gap(0.2, stem_gap);
        let t_regrab = d.t - 0.05;
        sounds.push((t_regrab - 0.05, Sound::Grip));
        let rel2 = arm::tip(&d.track.sample(t_regrab).q).inverse() * lying;
        letters[a].phases.push((t_regrab, Phase::Carried(rel2)));
        d.pose(
            0.55,
            stem_world + Vec3::Y * 0.4,
            grab_rot,
            stem_gap,
            Ease::Smooth,
            0.0,
        );
        // It stands the letter up in the air. Then it lowers the letter into its place.
        let slot_tip = letters[a].slot * rel2.inverse();
        let (_, slot_rot, slot_pos) = slot_tip.to_scale_rotation_translation();
        d.pose(
            1.0,
            slot_pos + Vec3::Y * 0.35,
            slot_rot,
            stem_gap,
            Ease::Smooth,
            0.0,
        );
        d.pose(0.5, slot_pos, slot_rot, stem_gap, Ease::Smooth, 0.0);
        let t_place = d.t;
        let slot = letters[a].slot;
        letters[a].phases.push((t_place, Phase::Still(slot)));
        sounds.push((t_place, Sound::Place));
        say(&mut log, t_place, "PLACE  a ............. OK", false);
        d.gap(0.12, OPEN);
        let back = slot_rot * Vec3::NEG_Z;
        d.pose(
            0.35,
            slot_pos + back * 0.2 + Vec3::Y * 0.05,
            slot_rot,
            OPEN,
            Ease::Out,
            0.0,
        );
        // Two pats on the letter.
        let top = letters[a].slot.transform_point3(letters[a].top);
        d.pose(0.45, top + Vec3::Y * 0.16, rot, 0.03, Ease::Smooth, 0.0);
        for _ in 0..2 {
            d.pose(0.1, top + Vec3::Y * 0.05, rot, 0.03, Ease::In, 0.0);
            sounds.push((d.t, Sound::Pat));
            d.pose(0.14, top + Vec3::Y * 0.16, rot, 0.03, Ease::Out, 0.0);
        }
        d.joints(
            0.25,
            [0.0, -0.05, -0.05, 0.0, -0.3, 0.0],
            Ease::OutBack(1.5),
            0.3,
        );
        sounds.push((d.t - 0.1, Sound::Happy));
        beats.finish = d.t;

        // ---------------------------------------------------------------- Confident again
        for (i, pace) in [(4usize, 0.62f32), (5, 0.58), (6, 0.56)] {
            let t = pick_place(&mut d, &mut letters, &mut sounds, i, pace, false);
            say(
                &mut log,
                t,
                match i {
                    4 => "PLACE  m ............. OK",
                    5 => "PLACE  e ............. OK",
                    _ => "PLACE  s ............. OK",
                },
                false,
            );
        }
        say(&mut log, d.t + 0.1, "TASK COMPLETE  1 INCIDENT", false);

        // ---------------------------------------------------------------- Admire, bow, ta-da
        let word_center = vec3(0.0, 0.25, crate::WORD_Z);
        d.look(
            0.7,
            vec3(0.0, 1.55, 0.55),
            word_center,
            0.0,
            Ease::Smooth,
            0.3,
        );
        d.hold(0.5, 0.5);
        d.look(
            0.5,
            vec3(0.0, 1.5, 0.75),
            FINAL_CAMERA,
            0.25,
            Ease::OutBack(1.2),
            0.3,
        );
        sounds.push((d.t - 0.1, Sound::Proud));
        d.hold(0.35, 0.4);
        // The bow.
        d.joints(0.45, [0.0, 0.22, 0.3, 0.0, 0.65, 0.0], Ease::Smooth, 0.0);
        d.hold(0.25, 0.0);
        d.joints(
            0.4,
            [0.0, -0.24, -0.32, 0.0, -0.7, 0.0],
            Ease::OutBack(1.3),
            0.2,
        );
        beats.tada = d.t;
        sounds.push((d.t, Sound::TaDa));
        say(&mut log, d.t, "HUMILITY  NOT FOUND", false);
        for _ in 0..2 {
            d.gap(0.09, 0.02);
            d.gap(0.09, OPEN);
        }
        blinks.push(d.t + 0.2);
        d.hold(1.7, 0.6);
        beats.end = d.t;

        sounds.sort_by(|a, b| a.0.total_cmp(&b.0));
        Story {
            track: d.track,
            letters,
            falls,
            sounds,
            log,
            blinks,
            beats,
            dropped: a,
        }
    }

    /// The eye lid at `t`: 1 is open, 0 is closed.
    pub fn lid(&self, t: f32) -> f32 {
        let awake = smoothstep(self.beats.wake + 0.6, self.beats.wake + 1.0, t);
        let blink = self
            .blinks
            .iter()
            .map(|b| 1.0 - smoothstep(0.0, 0.07, (t - b).abs()))
            .fold(0.0, f32::max);
        awake * (1.0 - blink)
    }
}
