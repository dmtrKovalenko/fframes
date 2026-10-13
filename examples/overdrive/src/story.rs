//! The choreography: everything that moves, as a function of the video time `t` in seconds.

use fframes_3d::math::{hash, progress, random_direction, smoothstep};
use fframes_3d::*;

use crate::robot::{Bone, PELVIS, Pose, Robot, RobotState, SCALE, packed_offset};

/// Length of the film in seconds.
pub const DURATION: f32 = 13.0;
/// One beat at 120 BPM. The car jumps on beat 4 and lands on beat 8. The robot fires on beat
/// 17, and the title comes on beat 22.
pub const BEAT: f32 = 0.5;

/// The car leaves the road.
pub const LAUNCH: f32 = 2.0;
/// The car transforms into the robot from `MORPH_START` to `MORPH_END`.
pub const MORPH_START: f32 = 2.08;
pub const MORPH_END: f32 = 3.6;
/// The robot lands.
pub const LAND: f32 = 4.0;
/// The robot stands up from `RISE` to `STOOD`.
pub const RISE: f32 = 4.55;
pub const STOOD: f32 = 5.6;
/// The robot powers up.
pub const POWER: f32 = 5.0;
/// The callouts come in.
pub const HUD: f32 = 5.5;
/// The robot aims the cannon.
pub const AIM: f32 = 7.0;
/// The cannon opens.
pub const CANNON: f32 = 7.25;
/// The cannon starts to charge.
pub const CHARGE: f32 = 7.5;
/// The cannon fires.
pub const FIRE: f32 = 8.5;
/// The robot crouches.
pub const CROUCH: f32 = 9.0;
/// The robot jumps forward.
pub const LEAP: f32 = 9.5;
/// The robot transforms back into the car from `REMORPH_START` to `REMORPH_END`.
pub const REMORPH_START: f32 = 9.58;
pub const REMORPH_END: f32 = 10.35;
/// The car lands on the road.
pub const CAR_LAND: f32 = 10.5;
/// The title comes in.
pub const TITLE: f32 = 11.0;

/// Speed of the car before the jump, in m/s.
const CAR_SPEED: f32 = 28.0;
/// Z of the car at the jump.
const LAUNCH_Z: f32 = -30.0;
/// Speed of the car after it lands again, in m/s.
const EXIT_SPEED: f32 = 30.0;
/// Z of the car when it lands again.
const EXIT_Z: f32 = 12.0;
/// The camera on the road that the car passes.
const ROAD_CAMERA: Vec3 = Vec3::new(2.6, 0.6, 20.0);
/// The time at which the car passes the road camera.
pub const PASS: f32 = CAR_LAND + (ROAD_CAMERA.z - EXIT_Z) / EXIT_SPEED;
/// The pelvis of the scaled skeleton, relative to its feet.
const CENTER: Vec3 = Vec3::new(0.0, PELVIS.y * SCALE, 0.0);

/// Cubic ease in and out.
fn ease_in_out(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x < 0.5 {
        4.0 * x * x * x
    } else {
        1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
    }
}

/// Cubic ease out.
fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

/// Cubic Hermite interpolation from `p0` with the tangent `m0` to `p1` with the tangent `m1`.
fn hermite(p0: f32, m0: f32, p1: f32, m1: f32, s: f32) -> f32 {
    let (s2, s3) = (s * s, s * s * s);
    (2.0 * s3 - 3.0 * s2 + 1.0) * p0
        + (s3 - 2.0 * s2 + s) * m0
        + (-2.0 * s3 + 3.0 * s2) * p1
        + (s3 - s2) * m1
}

/// Exponential decay after an impact at `at`, 0 before.
pub fn impact(t: f32, at: f32, decay: f32) -> f32 {
    if t < at {
        0.0
    } else {
        (-(t - at) * decay).exp()
    }
}

/// The root transform of the skeleton, turned by `pitch` about its pelvis at `center`.
fn flying_root(center: Vec3, pitch: f32) -> Mat4 {
    Mat4::from_translation(center)
        * Mat4::from_rotation_x(pitch)
        * Mat4::from_translation(-CENTER)
        * Mat4::from_scale(Vec3::splat(SCALE))
}

/// The pelvis position of the skeleton in the car at `car`.
fn packed_center(car: Vec3) -> Vec3 {
    (Mat4::from_translation(car) * packed_offset()).transform_point3(PELVIS)
}

/// The car transform at `position`, pitched by `pitch` radians.
fn car_matrix(position: Vec3, pitch: f32) -> Mat4 {
    Mat4::from_translation(position) * Mat4::from_rotation_x(pitch)
}

/// Z of the car on the road before the jump.
fn approach_z(t: f32) -> f32 {
    LAUNCH_Z - CAR_SPEED * (LAUNCH - t)
}

/// Z of the car on the road after the robot turns back into the car.
fn exit_z(t: f32) -> f32 {
    EXIT_Z + EXIT_SPEED * (t - CAR_LAND)
}

/// Pose measurements that the choreography needs, measured once.
pub struct Calibration {
    /// The lift that puts the landing pose on the floor.
    pub land_lift: f32,
    /// The lift that puts the crouch before the jump on the floor.
    pub crouch_lift: f32,
}

impl Calibration {
    pub fn measure(robot: &Robot, scene: &Scene) -> Self {
        let mut scratch = scene.clone();
        let root = Mat4::from_scale(Vec3::splat(SCALE));
        Self {
            land_lift: robot.ground_offset(&mut scratch, root, &Pose::landing()),
            crouch_lift: robot.ground_offset(&mut scratch, root, &Pose::crouch()),
        }
    }
}

/// The robot state at `t`, and true when the robot stands on the ground.
pub fn robot_at(t: f32, cal: &Calibration) -> (RobotState, bool) {
    let mut s = RobotState {
        car: Mat4::IDENTITY,
        root: Mat4::IDENTITY,
        pose: Pose::packed(),
        morph: 0.0,
        wheel_spin: 0.0,
        eyes: 0.0,
        reactor: 0.0,
        seams: 0.0,
        headlights: 7.0,
        taillights: 6.0,
        cannon: 0.0,
        charge: 0.0,
        chest_open: 0.0,
    };
    let mut grounded = false;

    if t < LAUNCH {
        // The car races down the road with a small suspension bounce.
        let z = approach_z(t);
        let bounce = 0.015 * (t * 31.0).sin() + 0.01 * (t * 17.0).sin();
        s.car = car_matrix(vec3(0.0, bounce, z), 0.004 * (t * 23.0).sin());
        s.root = s.car * packed_offset();
        s.wheel_spin = z / 0.36 * 0.12;
    } else if t < LAND {
        // Bullet time: the car leaves the road, flips forward and unfolds as it falls.
        let flight = LAND - LAUNCH;
        let x = (t - LAUNCH) / flight;
        let start = packed_center(vec3(0.0, 0.0, LAUNCH_Z));
        let end = CENTER + vec3(0.0, cal.land_lift, 0.0) + Pose::landing().pelvis * SCALE;
        let z = hermite(start.z, CAR_SPEED * flight, end.z, 2.0 * flight, x);
        let y = start.y
            + (end.y - start.y) * smoothstep(0.0, 1.0, x)
            + 4.2 * 4.0 * x * (1.0 - x) * (1.0 - 0.25 * x);
        let pitch = deg(90.0) + deg(270.0) * ease_in_out(progress(t, LAUNCH + 0.05, LAND - 0.12));
        s.root = flying_root(vec3(0.0, y, z), pitch);
        s.car = s.root * packed_offset().inverse();
        s.morph = ease_in_out(progress(t, MORPH_START, MORPH_END));
        let p = progress(t, LAUNCH, LAND - 0.05);
        s.pose = if p < 0.45 {
            Pose::packed().lerp(&Pose::tuck(), ease_in_out(p / 0.45))
        } else {
            Pose::tuck().lerp(&Pose::landing(), ease_in_out((p - 0.45) / 0.55))
        };
        // In the air the root holds the pelvis offset. The pose must not add it again.
        s.pose.pelvis = Vec3::ZERO;
        s.wheel_spin = approach_z(LAUNCH) / 0.36 + (t - LAUNCH) * 40.0 * (1.0 - x);
        s.headlights = 7.0 * (1.0 - s.morph);
        s.taillights = 6.0 * (1.0 - s.morph * 0.6);
        s.seams = 4.0 * smoothstep(0.3, 1.0, s.morph);
    } else if t < LEAP {
        grounded = true;
        s.morph = 1.0;
        s.root = Mat4::from_scale(Vec3::splat(SCALE));
        // The landing pose, pushed down by the impact. Then the robot stands up.
        let squash = impact(t, LAND, 9.0) * (1.0 - (-(t - LAND) * 40.0).exp());
        let landing =
            Pose::landing().pelvis(Pose::landing().pelvis - vec3(0.0, 0.12 * squash, 0.0));
        let rise = ease_in_out(progress(t, RISE, STOOD));
        let mut pose = landing.lerp(&Pose::stand(), rise);
        // Breathing.
        pose.joints[Bone::Chest as usize] *= Quat::from_rotation_x(0.025 * (t * 2.6).sin());
        // Aim the cannon.
        let aim = ease_in_out(progress(t, AIM, AIM + 0.5))
            * (1.0 - ease_in_out(progress(t, FIRE + 0.25, CROUCH)));
        pose = pose.lerp(&Pose::aim(), aim);
        // Recoil.
        let recoil = impact(t, FIRE, 7.0) * smoothstep(FIRE, FIRE + 0.03, t);
        pose = pose
            .with_extra(Bone::ShoulderR, -recoil * 0.12)
            .with_extra(Bone::Chest, -recoil * 0.12);
        // Crouch before the jump.
        let crouch = ease_in_out(progress(t, CROUCH, LEAP));
        s.pose = pose.lerp(&Pose::crouch(), crouch);
        s.cannon = ease_in_out(progress(t, CANNON, CANNON + 0.5))
            * (1.0 - ease_in_out(progress(t, FIRE + 0.3, CROUCH)));
        s.charge = 10.0 * smoothstep(CHARGE, FIRE, t) * (1.0 - smoothstep(FIRE, FIRE + 0.5, t))
            + 40.0 * impact(t, FIRE, 12.0);
        s.headlights = 0.0;
        s.taillights = 3.0;
    } else {
        // Jump forward, dive and fold back into the car.
        let flight = CAR_LAND - LEAP;
        let x = progress(t, LEAP, CAR_LAND);
        let start = CENTER + vec3(0.0, cal.crouch_lift, 0.0) + Pose::crouch().pelvis * SCALE;
        let end = packed_center(vec3(0.0, 0.0, EXIT_Z));
        if t < CAR_LAND {
            let z = hermite(start.z, 4.0 * flight, end.z, EXIT_SPEED * flight, x);
            let y = start.y + (end.y - start.y) * x + 2.6 * 4.0 * x * (1.0 - x);
            let pitch = deg(90.0) * ease_in_out(progress(t, LEAP, CAR_LAND - 0.1));
            s.root = flying_root(vec3(0.0, y, z), pitch);
            s.car = s.root * packed_offset().inverse();
            let p = progress(t, LEAP, CAR_LAND - 0.1);
            s.pose = Pose::crouch().lerp(&Pose::packed(), ease_in_out(p));
            s.pose.pelvis = Vec3::ZERO;
            s.morph = 1.0 - ease_in_out(progress(t, REMORPH_START, REMORPH_END));
            s.wheel_spin = (t - REMORPH_START).max(0.0) * 60.0;
        } else {
            let z = exit_z(t);
            let settle = impact(t, CAR_LAND, 10.0) * (t * 40.0).sin() * 0.04;
            s.car = car_matrix(vec3(0.0, -settle.abs(), z), settle * 0.3);
            s.root = s.car * packed_offset();
            s.wheel_spin = (CAR_LAND - REMORPH_START) * 60.0 + (z - EXIT_Z) / 0.36 * 0.12;
        }
        s.headlights = 7.0 * (1.0 - s.morph);
        s.taillights = 6.0 + 4.0 * smoothstep(CAR_LAND, CAR_LAND + 0.3, t);
        s.seams = 4.0 * s.morph;
    }

    // Power up after the landing.
    if (LAND - 0.05..LEAP + 0.5).contains(&t) {
        let flicker = if t < POWER {
            0.0
        } else if t < POWER + 0.25 {
            f32::from(u8::from(hash(((t - POWER) * 60.0) as u32) > 0.4))
        } else {
            1.0
        };
        let pulse = 1.0 + 0.25 * ((t - POWER) * std::f32::consts::TAU * 2.0).sin().max(0.0);
        s.eyes =
            9.0 * flicker * pulse + 3.0 * smoothstep(LAND - 0.05, LAND + 0.2, t) * (1.0 - flicker);
        s.reactor = 7.0 * smoothstep(POWER, POWER + 0.6, t) * pulse;
        s.seams =
            4.0 * smoothstep(POWER + 0.1, POWER + 0.7, t) * pulse + 2.0 * impact(t, LAND, 4.0);
        s.chest_open = ease_out(progress(t, POWER, POWER + 0.4))
            * (1.0 - ease_in_out(progress(t, CROUCH, LEAP)));
        if t > LEAP {
            let fade = 1.0 - s.morph;
            s.eyes *= 1.0 - fade;
            s.reactor *= 1.0 - fade;
        }
    } else if (MORPH_START..LAND).contains(&t) {
        s.eyes = 2.0 * smoothstep(0.7, 1.0, s.morph);
    }
    (s, grounded)
}

/// The pelvis of the robot, as a target for the cameras.
pub fn center_of(state: &RobotState) -> Vec3 {
    state.root.transform_point3(PELVIS)
}

/// The cut list: the camera at `t`.
pub fn camera_at(t: f32, robot: &RobotState) -> Camera {
    let car = robot.car.w_axis.truncate();
    let lens = Camera::perspective(38.0);
    if t < 1.0 {
        // Low, in front of the car. The car comes nearer.
        let ahead = 8.5 - 3.0 * t;
        lens.fov(34.0)
            .position(car + vec3(1.7, 0.55, ahead))
            .look_at(car + vec3(0.0, 0.75, 0.4))
            .roll(deg(-4.0))
    } else if t < LAUNCH {
        // Near the front wheel, with a tilted horizon.
        let x = t - 1.0;
        lens.fov(34.0)
            .position(car + vec3(5.6 - 0.6 * x, 0.75, 3.4 - 3.6 * x))
            .look_at(car + vec3(0.0, 0.6, 0.3 - 0.8 * x))
            .roll(deg(6.0))
    } else if t < LAND - 0.15 {
        // Bullet time: an orbit around the flip.
        let c = center_of(robot);
        let x = progress(t, LAUNCH, LAND - 0.15);
        let ease = ease_in_out(x);
        lens.fov(40.0)
            .orbit(c, 9.5 - 2.5 * ease, 70.0 - 200.0 * ease, 6.0 + 10.0 * ease)
            .roll(deg(6.0) * (1.0 - ease))
    } else if t < LAND + 0.85 {
        // The landing: wide and low. The impact shakes the camera.
        let shake = 0.06 * impact(t, LAND, 5.0);
        lens.fov(36.0)
            .position(vec3(3.2, 0.9, 11.0 - 0.6 * (t - LAND)))
            .look_at(vec3(0.0, 1.7, 0.0))
            .shake(t, shake, 3)
    } else if t < AIM {
        // Hero shot: from the feet up to the face, with a slow push in.
        let x = ease_in_out(progress(t, LAND + 0.85, 6.6));
        let drift = progress(t, 6.6, AIM);
        let low = vec3(1.4, 0.35, 5.6);
        let high = vec3(1.6, 2.4, 9.0);
        lens.fov(34.0)
            .position(low.lerp(high, x) + vec3(-0.8 * drift, 0.1 * drift, -0.4 * drift))
            .look_at(vec3(0.0, 2.0 + 0.5 * x, 0.0))
            .shake(t, 0.004, 8)
    } else if t < 8.2 {
        // Over the right shoulder while the cannon opens.
        let x = progress(t, AIM, 8.2);
        lens.fov(36.0)
            .position(vec3(-7.4 + 0.8 * x, 2.5 + 0.3 * x, 1.2 - 0.6 * x))
            .look_at(vec3(-0.6, 2.7, 1.4))
            .roll(deg(-2.0))
    } else if t < CROUCH {
        // Along the barrel.
        let shake = 0.04 * impact(t, FIRE, 6.0);
        let push = 0.5 * ease_out(progress(t, FIRE, FIRE + 0.4));
        lens.fov(38.0)
            .position(vec3(-4.6 - push * 0.3, 2.3, 6.4 + push))
            .look_at(vec3(-0.4, 2.6, 2.6))
            .shake(t, shake + 0.003, 5)
    } else if t < TITLE - 0.08 {
        // Low on the road. The robot jumps at the camera, and the car passes it.
        let c = center_of(robot);
        let target = vec3(0.0, 1.2, 0.0).lerp(c, 0.85);
        lens.fov(40.0)
            .position(ROAD_CAMERA)
            .look_at(target + vec3(0.0, 0.4, 0.0))
            .shake(t, 0.02 * impact(t, CAR_LAND, 6.0), 9)
    } else {
        // The tail lights go away into the fog.
        lens.fov(34.0)
            .position(vec3(-1.2, 1.1, 24.0 + (t - TITLE) * 2.0))
            .look_at(vec3(0.0, 0.9, 80.0))
    }
}

/// A thing that happens once. The sound and the effects use the events.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    pub time: f32,
    pub kind: EventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// A panel locks into place.
    Clank,
    Launch,
    Land,
    PowerUp,
    Blip,
    CannonOpen,
    Charge,
    Fire,
    Leap,
    CarLand,
    PassBy,
    Title,
}

/// The time at which each panel locks, in both transformations.
pub fn panel_locks(robot: &Robot) -> Vec<(f32, usize)> {
    let mut out = Vec::new();
    for (i, panel) in robot.panels.iter().enumerate() {
        // `Robot::apply` moves a panel while the morph goes from `delay` to `delay + 0.42`.
        let lock = (panel.delay + 0.42).min(1.0);
        out.push((morph_time(lock), i));
        let unlock = panel.delay;
        out.push((remorph_time(unlock), i));
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// The video time at which the forward morph reaches `m`.
fn morph_time(m: f32) -> f32 {
    MORPH_START + inverse_ease(m) * (MORPH_END - MORPH_START)
}

/// The video time at which the backward morph comes down to `m`.
fn remorph_time(m: f32) -> f32 {
    REMORPH_START + inverse_ease(1.0 - m) * (REMORPH_END - REMORPH_START)
}

/// The inverse of [`ease_in_out`], by bisection.
fn inverse_ease(y: f32) -> f32 {
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..30 {
        let mid = f32::midpoint(lo, hi);
        if ease_in_out(mid) < y {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    f32::midpoint(lo, hi)
}

/// Every event of the film, in time order.
pub fn events(robot: &Robot) -> Vec<Event> {
    let mut events: Vec<Event> = Vec::new();
    // Locks less than 30 ms after the last clank share that clank.
    for (time, _) in panel_locks(robot) {
        if events.last().is_none_or(|e: &Event| time - e.time > 0.03) {
            events.push(Event {
                time,
                kind: EventKind::Clank,
            });
        }
    }
    events.extend([
        Event {
            time: LAUNCH,
            kind: EventKind::Launch,
        },
        Event {
            time: LAND,
            kind: EventKind::Land,
        },
        Event {
            time: POWER,
            kind: EventKind::PowerUp,
        },
        Event {
            time: CANNON,
            kind: EventKind::CannonOpen,
        },
        Event {
            time: CHARGE,
            kind: EventKind::Charge,
        },
        Event {
            time: FIRE,
            kind: EventKind::Fire,
        },
        Event {
            time: LEAP,
            kind: EventKind::Leap,
        },
        Event {
            time: CAR_LAND,
            kind: EventKind::CarLand,
        },
        Event {
            time: PASS - 0.3,
            kind: EventKind::PassBy,
        },
        Event {
            time: TITLE,
            kind: EventKind::Title,
        },
    ]);
    for i in 0..4 {
        events.push(Event {
            time: HUD + 0.3 + i as f32 * 0.35,
            kind: EventKind::Blip,
        });
    }
    events.sort_by(|a, b| a.time.total_cmp(&b.time));
    events
}

/// A burst of sparks.
pub struct SparkBurst {
    pub time: f32,
    pub position: Vec3,
    /// The velocity of the part that throws the sparks.
    pub velocity: Vec3,
    pub count: u32,
    pub speed: f32,
    pub seed: u32,
    pub color: Rgb,
}

/// The spark particles at `t`.
pub fn sparks_at(t: f32, bursts: &[SparkBurst]) -> Vec<Particle> {
    let gravity = vec3(0.0, -9.8, 0.0);
    let mut out = Vec::new();
    for burst in bursts {
        let age = t - burst.time;
        if !(0.0..0.7).contains(&age) {
            continue;
        }
        for i in 0..burst.count {
            let seed = burst.seed.wrapping_mul(977).wrapping_add(i);
            let life = 0.2 + 0.4 * hash(seed * 3);
            if age > life {
                continue;
            }
            let dir = (random_direction(seed) + vec3(0.0, 0.6, 0.0)).normalize();
            let v0 = dir * burst.speed * (0.4 + 0.8 * hash(seed * 5)) + burst.velocity;
            let mut p = burst.position + v0 * age + gravity * (0.5 * age * age);
            let v = v0 + gravity * age;
            p.y = p.y.max(0.02);
            let fade = 1.0 - age / life;
            out.push(
                Particle::new(
                    p,
                    0.012 + 0.01 * hash(seed * 7),
                    burst.color * (14.0 * fade * fade + 1.0),
                )
                .tail(-v * 0.022)
                .opacity(fade),
            );
        }
    }
    out
}
