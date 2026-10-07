//! Independently rotating pixel objects in camera space. Camera moves and ring
//! deformation are sampled from fframes timelines once, when the video is built.
use std::f32::consts::{PI, TAU};

use fframes::{FFramesContext, Frame, Shader, ShaderUniforms, Svgr, animation::Easing};

type V3 = [f32; 3];

#[derive(Clone, Copy)]
pub(super) struct RingExcursion {
    pub start: usize,
    departure: usize,
    pub arrival: usize,
    pub return_start: usize,
    return_end: usize,
}

// Each return finishes before the next object leaves. The wheel keeps its
// position and angular phase even while hidden behind the feature cards.
pub(super) const EXCURSIONS: [RingExcursion; 3] = [
    RingExcursion {
        start: 200,
        departure: 190,
        arrival: 205,
        return_start: 214,
        return_end: 225,
    },
    RingExcursion {
        start: 237,
        departure: 227,
        arrival: 242,
        return_start: 249,
        return_end: 259,
    },
    RingExcursion {
        start: 265,
        departure: 261,
        arrival: 270,
        return_start: 280,
        return_end: 294,
    },
];

fn turn(mut p: V3, a: usize, b: usize, angle: f32) -> V3 {
    let (s, c) = angle.sin_cos();
    (p[a], p[b]) = (c * p[a] - s * p[b], s * p[a] + c * p[b]);
    p
}

#[derive(Clone, Debug)]
pub(super) struct Object {
    icon: usize,
    center: V3,
    right: V3,
    down: V3,
    size: f32,
    stretch: [f32; 2],
    heat: f32,
    soot: f32,
    pixels: f32,
    angles: V3,
    bounds: [f32; 4],
}

impl Object {
    // Shared camera-space interpolation keeps the selected ring object alive
    // through the edit; rebuilding a separate hero entrance caused the jump.
    fn handoff(&self, target: &Self, progress: f32, lift: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * progress;
        let mut center = std::array::from_fn(|i| mix(self.center[i], target.center[i]));
        center[1] -= (progress * PI).sin() * lift;
        let mut object = Self::new(
            self.icon,
            center,
            mix(self.size, target.size),
            std::array::from_fn(|i| mix(self.angles[i], target.angles[i])),
            mix(self.heat, target.heat),
        );
        object.soot = mix(self.soot, target.soot);
        object.pixels = mix(self.pixels, target.pixels);
        object.with_stretch(std::array::from_fn(|i| {
            mix(self.stretch[i], target.stretch[i])
        }))
    }

    // Continue the feature's last measured velocity through the background
    // cut. The return destination is sampled separately from the real slot.
    fn continued(&self, previous: &Self, elapsed: f32) -> Self {
        let extend = |a: f32, b: f32| a + (a - b) * elapsed;
        let mut object = Self::new(
            self.icon,
            std::array::from_fn(|i| extend(self.center[i], previous.center[i])),
            extend(self.size, previous.size),
            std::array::from_fn(|i| extend(self.angles[i], previous.angles[i])),
            self.heat,
        );
        object.soot = self.soot;
        object.pixels = self.pixels;
        object.with_stretch(self.stretch)
    }

    fn foreground(&self, amount: f32) -> Self {
        // Scale camera position and physical size together: the projected pose
        // is identical, but the flying icon clears the other ring objects.
        let ratio = 1. + (550_f32.min(self.center[2]) / self.center[2] - 1.) * amount;
        let mut object = Self::new(
            self.icon,
            self.center.map(|value| value * ratio),
            self.size * ratio,
            self.angles,
            self.heat,
        );
        object.soot = self.soot;
        object.pixels = self.pixels;
        object.with_stretch(self.stretch)
    }

    pub(super) fn new(icon: usize, center: V3, size: f32, angles: V3, heat: f32) -> Self {
        let rotate = |p| {
            turn(
                turn(turn(p, 0, 2, angles[0]), 1, 2, angles[1]),
                0,
                1,
                angles[2],
            )
        };
        let right = rotate([1., 0., 0.]);
        let down = rotate([0., 1., 0.]);
        Self {
            icon,
            center,
            right,
            down,
            size,
            stretch: [1.; 2],
            heat,
            soot: 0.,
            pixels: 32.,
            angles,
            bounds: [0.; 4],
        }
        .with_stretch([1.; 2])
    }

    pub(super) fn with_stretch(mut self, stretch: [f32; 2]) -> Self {
        self.stretch = stretch;
        let mut low = [1440_f32, 1080_f32];
        let mut high = [0_f32, 0_f32];
        for (x, y) in [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)] {
            let p: V3 = std::array::from_fn(|i| {
                self.center[i]
                    + (x * self.right[i] * stretch[0] + y * self.down[i] * stretch[1])
                        * self.size
                        * 0.5
            });
            let screen = [720. + 1250. * p[0] / p[2], 540. + 1250. * p[1] / p[2]];
            for i in 0..2 {
                low[i] = low[i].min(screen[i]);
                high[i] = high[i].max(screen[i]);
            }
        }
        let x = (low[0] - 2.).floor().max(0.);
        let y = (low[1] - 2.).floor().max(0.);
        let w = (high[0] + 2.).ceil().min(1440.) - x;
        let h = (high[1] + 2.).ceil().min(1080.) - y;
        self.bounds = [x, y, w, h];
        self
    }

    pub(super) fn draw(
        &self,
        frame: &Frame,
        ctx: &FFramesContext<'_, '_>,
        shader: &Shader,
    ) -> Svgr<'static> {
        let Some(atlas) = ctx.get_image("objects.png") else {
            return Svgr::empty();
        };
        let [x, y, w, h] = self.bounds;
        if w <= 0. || h <= 0. {
            return Svgr::empty();
        }
        let layer = shader.draw(
            frame,
            ShaderUniforms::new()
                .image("uAtlas", atlas)
                .float2("uOrigin", x, y)
                .float3("uCenter", self.center[0], self.center[1], self.center[2])
                .float3("uRight", self.right[0], self.right[1], self.right[2])
                .float3("uDown", self.down[0], self.down[1], self.down[2])
                .float("uSize", self.size)
                .float2("uStretch", self.stretch[0], self.stretch[1])
                .float2(
                    "uCell",
                    (self.icon % 4) as f32 * 64.,
                    (self.icon / 4) as f32 * 64.,
                )
                .float("uHeat", self.heat)
                .float("uSoot", self.soot)
                .float("uPixels", self.pixels),
        );
        fframes::svgr!(<image href={layer.href()} x={x} y={y} width={w} height={h} />)
    }
}

/// Replace one permanent slot's occupant with a complete excursion, including
/// the feature hold. Feature cards show only the selected track; the hidden
/// wheel keeps rotating so its vacant slot remains the exact return target.
pub(super) fn ring_excursion(
    frames: &mut [Vec<Object>],
    heroes: &mut [Object],
    schedule: RingExcursion,
    keep_spinning: bool,
) {
    let RingExcursion {
        start,
        departure,
        arrival,
        return_start,
        return_end,
    } = schedule;
    let outward_seconds = (arrival - departure) as f32 / 24.;
    let return_seconds = (return_end - return_start) as f32 / 24.;
    let pull = fframes::timeline!(
        at 0.0 => outward_seconds, animate 0.0_f32 => 1.0, Easing::CubicBezier(0.42,0.,0.58,1.),
    );
    let rejoin = fframes::timeline!(
        at 0.0 => return_seconds, animate 0.0_f32 => 1.0, Easing::CubicBezier(0.42,0.,0.58,1.),
    );
    // All feature poses live in front of the wheel, with the same projected
    // size as the authored card. Flight depth now interpolates continuously.
    let original = heroes
        .iter()
        .map(|pose| pose.foreground(1.))
        .collect::<Vec<_>>();
    let target = original[arrival - start].clone();
    let icon = target.icon;
    let end = start + heroes.len();
    let Some(destination_roll) = frames[return_end]
        .iter()
        .find(|object| object.icon == icon)
        .map(|object| object.angles[2])
    else {
        return;
    };
    let roll_offset = if keep_spinning {
        ((original[return_start - start].angles[2] - destination_roll) / TAU).ceil() * TAU
    } else {
        0.
    };
    for (index, objects) in frames
        .iter_mut()
        .enumerate()
        .take(return_end + 1)
        .skip(departure)
    {
        if let Some(selected) = objects.iter_mut().find(|object| object.icon == icon) {
            // This exact, still-moving slot exists for every frame, including
            // the black cards. Replace its occupant once; never draw a duplicate
            // in the wheel, and never reconstruct its location across a cut.
            let mut slot = selected.clone();
            let pose = if index <= arrival {
                let frame = Frame::new(index - departure, index, 24);
                slot.handoff(&target, frame.animate(&pull), 90.)
            } else if index < return_start {
                original[index - start].clone()
            } else {
                let source = if index < end {
                    original[index - start].clone()
                } else {
                    original[original.len() - 1]
                        .continued(&original[original.len() - 2], (index + 1 - end) as f32)
                };
                slot.angles[2] += roll_offset;
                let frame = Frame::new(index - return_start, index, 24);
                source.handoff(&slot, frame.animate(&rejoin), -55.)
            };
            *selected = pose.clone();
            if (start..end).contains(&index) {
                heroes[index - start] = pose;
            }
        }
        objects.sort_by(|a, b| b.center[2].total_cmp(&a.center[2]));
    }
}

pub(super) fn hero(icon: usize, x: f32, y: f32, size: f32, time: f32) -> Object {
    Object::new(
        icon,
        [x - 720., y - 540., 1250.],
        size,
        [
            (time * 6.0 + 0.4).sin() * 0.8,
            0.15 * (time * 4.).cos(),
            -0.10 + (time * 2.7).sin() * 0.25,
        ],
        0.,
    )
}

fn base_choreography() -> Vec<Vec<Object>> {
    let tilt = fframes::timeline!(
        at 6.541_667 => 7.291_667, animate 0.34_f32 => 0.63, Easing::CubicBezier(0.65,0.,0.35,1.),
        at 7.291_667 => 7.833_333, animate 0.63_f32 => 0.50, Easing::EaseInOut,
        at 7.833_333 => 8.333_333, animate 0.50_f32 => 0.48, Easing::EaseInOut,
        at 11.916_667 => 12.5, animate 0.48_f32 => 0.98, Easing::CubicBezier(0.16,1.,0.3,1.),
    );
    let explode = fframes::timeline!(
        at 12.583_333 => 13.0, animate 0.0_f32 => 1.0, Easing::CubicBezier(0.16,1.,0.3,1.),
    );
    // Arrange the three featured objects along the approaching arc. At their
    // departure frames (190, 227, 261), each sits just left of the nearest
    // point, rather than being lifted through the ring from its far side.
    // Keep angular velocity continuous across the hero/background cuts.
    let front_angle = 1.95;
    let departure_rotation = front_angle + PI * 0.5 - 2. * TAU / 12.;
    let scatter_rotation = departure_rotation + (302. - 190.) / 24.;
    let spin = fframes::timeline!(
        at (153.0 / 24.0) => (302.0 / 24.0),
            animate (departure_rotation + (153. - 190.) / 24.) => scatter_rotation,
            Easing::Linear,
    );
    let order = [10, 7, 0, 6, 11, 9, 3, 2, 1, 4, 8, 5];
    (0..489)
        .map(|index| {
            let frame = Frame::new(index, index, 24);
            let t = frame.seconds();
            let open = frame.animate(&explode);
            let plane = frame.animate(&tilt);
            let rotation = frame.animate(&spin);
            let mut objects = Vec::with_capacity(12);
            if (48..93).contains(&index) {
                let positions = [[735., 351.], [1060., 348.], [960., 665.], [1120., 869.]];
                for (i, icon) in [7, 1, 11, 0].into_iter().enumerate() {
                    let local = (index - 48) as f32;
                    let phase = local * 0.11 + i as f32 * 2.3;
                    let x = positions[i][0] + phase.sin() * 15.;
                    let y = positions[i][1] + (phase * 0.8).cos() * 13.;
                    let mut object = Object::new(
                        icon,
                        [x - 720., y - 540., 1250.],
                        104.,
                        [(phase * 0.82).sin() * 0.68, 0.12, phase.sin() * 0.16],
                        0.,
                    );
                    // The source briefly breaks these distant objects down
                    // into big mosaic pixels, then resolves their detail again.
                    object.pixels = match index {
                        48..=54 => 16.,
                        75..=77 => 12.,
                        78..=81 => 6.,
                        82..=84 => 10.,
                        _ => 32.,
                    };
                    objects.push(object);
                }
            } else if (153..331).contains(&index) {
                for (i, icon) in order.into_iter().enumerate() {
                    let a = i as f32 * TAU / 12. + rotation - PI * 0.5;
                    let (sin, cos) = a.sin_cos();
                    let radius = 355.;
                    let mut x = cos * radius;
                    let mut y = sin * radius * plane;
                    let mut z = 1250. - sin * 600. * (1. - plane * plane).sqrt();
                    // Explosion retains tangential momentum. Individual pieces then
                    // drift and tumble; it is not a flat scaling of the whole ring.
                    if open > 0. {
                        let a0 = i as f32 * TAU / 12. + scatter_rotation - PI * 0.5;
                        let targets = [
                            [0., -420.],
                            [150., -100.],
                            [420., -280.],
                            [350., 140.],
                            [530., 100.],
                            [520., 340.],
                            [-80., 30.],
                            [-360., 350.],
                            [-480., -130.],
                            [-360., -50.],
                            [-240., 70.],
                            [180., 200.],
                        ];
                        let drift = (t - 13.).max(0.);
                        x = a0.cos() * radius * (1. - open)
                            + targets[i][0] * open
                            + drift * 45. * a0.sin();
                        y = a0.sin() * radius * 0.98 * (1. - open)
                            + targets[i][1] * open
                            + drift * 32. * a0.cos();
                        z = 1250. + open * (i as f32 * 3.7).sin() * 140.;
                    }
                    let roll = (t * 0.9 + i as f32).sin() * 0.12
                        + open * (t - 12.58) * (i as f32 - 5.) * 0.17;
                    let yaw = (t * 1.05 + i as f32 * 1.7).sin() * 0.35 + open * (t - 12.58) * 0.75;
                    let heat = if index < 157 {
                        1. - (index - 153) as f32 * 0.14
                    } else if icon == 3 || icon == 5 {
                        ((t * 3. + i as f32).sin() - 0.85).max(0.) * 4.
                    } else {
                        0.
                    };
                    objects.push(Object::new(
                        icon,
                        [x, y, z],
                        195. - open * 35.,
                        [yaw, 0.1, roll],
                        heat,
                    ));
                }
            }
            objects.sort_by(|a, b| b.center[2].total_cmp(&a.center[2]));
            objects
        })
        .collect()
}

#[derive(Debug, Clone)]
pub(super) struct Impact {
    pub at: usize,
    pub position: [f32; 2],
    pub direction: [f32; 2],
    icon: usize,
    approach: [f32; 2],
}

pub(super) fn choreography() -> (Vec<Vec<Object>>, Vec<Impact>) {
    let mut frames = base_choreography();
    let mut impacts = Vec::new();
    // Pick the object at each reference contact point so the nib, flare and
    // response all use one event. This remains correct if the ring is retimed.
    for (at, desired, direction) in [
        (172, [1045., 592.], [0.65, 0.6]),
        (177, [767., 659.], [0.0, 1.0]),
        (182, [425., 671.], [-0.75, 0.65]),
        (188, [340., 487.], [-0.91, -0.41]),
        (312, [1080., 575.], [0.45, 0.9]),
        (318, [400., 878.], [-0.9, -0.1]),
    ] {
        let nearest = frames[at]
            .iter()
            .filter(|o| impacts.last().is_none_or(|h: &Impact| h.icon != o.icon))
            .min_by(|a, b| {
                let distance = |o: &Object| {
                    let x = 720. + o.center[0] * 1250. / o.center[2];
                    let y = 540. + o.center[1] * 1250. / o.center[2];
                    (x - desired[0]).powi(2) + (y - desired[1]).powi(2)
                };
                distance(a).total_cmp(&distance(b))
            });
        if let Some(object) = nearest {
            impacts.push(Impact {
                at,
                icon: object.icon,
                direction,
                position: desired,
                approach: [
                    (desired[0] - 720.) * object.center[2] / 1250. - object.center[0],
                    (desired[1] - 540.) * object.center[2] / 1250. - object.center[1],
                ],
            });
        }
    }
    let response = fframes::timeline!(
        at 0.0 => 0.083_333, animate 0.0_f32 => 1.0, Easing::EaseOut,
        at 0.083_333, animate 1.0_f32 => 0.0,
            Easing::Spring { mass: 1.0, stiffness: 190.0, damping: 14.0 },
    );
    let approach = fframes::timeline!(
        at 0.0 => 0.125, animate 0.0_f32 => 1.0, Easing::EaseInOut,
        at 0.125 => 0.75, animate 1.0_f32 => 0.0, Easing::EaseOut,
    );
    for (index, objects) in frames.iter_mut().enumerate() {
        for object in objects.iter_mut() {
            let mut center = object.center;
            let mut angles = object.angles;
            let mut heat = object.heat;
            let mut soot = 0.0_f32;
            for hit in &impacts {
                if hit.icon != object.icon || index + 3 < hit.at || index > hit.at + 25 {
                    continue;
                }
                let arrive = Frame::new(index + 3 - hit.at, index, 24).animate(&approach);
                center[0] += hit.approach[0] * arrive;
                center[1] += hit.approach[1] * arrive;
                if index < hit.at {
                    continue;
                }
                let elapsed = index - hit.at;
                let kick = Frame::new(elapsed, index, 24).animate(&response);
                center[0] += hit.direction[0] * 98. * kick;
                center[1] += hit.direction[1] * 72. * kick;
                center[2] -= 85. * kick;
                angles[0] += kick * 0.84;
                angles[2] += kick * hit.direction[0] * 0.43;
                heat = heat.max((1. - elapsed as f32 / 5.).max(0.) * 0.83);
                soot = soot.max(
                    ((elapsed as f32 - 3.) / 4.).clamp(0., 1.)
                        * (1. - (elapsed as f32 - 11.).max(0.) / 12.).max(0.)
                        * 0.92,
                );
            }
            let pixels = object.pixels;
            *object = Object::new(object.icon, center, object.size, angles, heat);
            object.soot = soot;
            object.pixels = pixels;
        }
        objects.sort_by(|a, b| b.center[2].total_cmp(&a.center[2]));
    }
    (frames, impacts)
}
