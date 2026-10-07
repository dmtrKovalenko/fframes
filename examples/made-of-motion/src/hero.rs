//! Cached hero poses share the ring trajectories; measured vector ink supplies the gestures.
use fframes::{FFramesContext, Frame, Svgr, Transform, animation::Easing};

use crate::{Studio, label, objects, restoration_capture};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Card {
    Code,
    Motion,
    Feeling,
}

#[derive(Debug)]
struct Exposure {
    object: objects::Object,
    title_opacity: f32,
}

#[derive(Debug)]
pub(super) struct Heroes {
    code: Vec<Exposure>,
    feeling: Vec<Exposure>,
    motion: Vec<Exposure>,
}

impl Heroes {
    pub(super) fn new(objects: &mut [Vec<objects::Object>]) -> Self {
        let arrival = fframes::timeline!(
            at 0.0 => 0.208_333, animate 0.0_f32 => 1.0, Easing::CubicBezier(0.12,0.9,0.22,1.),
        );
        let push = fframes::timeline!(
            at 0.208_333 => 0.583_333, animate 0.0_f32 => 0.25, Easing::Linear,
            at 0.583_333 => 0.875, animate 0.25_f32 => 1.0, Easing::EaseIn,
        );
        let yaw = fframes::timeline!(
            at 0.0 => 0.166_667, animate -0.85_f32 => -0.28, Easing::CubicBezier(0.16,1.,0.3,1.),
            at 0.166_667 => 0.833_333, animate -0.28_f32 => 0.40, Easing::EaseInOut,
        );
        let spin = fframes::timeline!(
            at 0.0 => 0.875, animate -0.38_f32 => 2.95, Easing::Linear,
        );
        let build = |feeling| {
            (0..if feeling { 21 } else { 20 })
                .map(|index| {
                    let frame = Frame::new(index, index, 24);
                    let entry = frame.animate(&arrival);
                    let push = frame.animate(&push);
                    let yaw = frame.animate(&yaw);
                    let object = objects::Object::new(
                        if feeling { 4 } else { 0 },
                        [
                            442. - 720. - (1. - entry) * 74.,
                            -9. + (1. - entry) * 68.,
                            1250. - push * 70.,
                        ],
                        (if feeling { 555. } else { 530. }) * (0.82 + entry * 0.18),
                        [
                            if feeling { yaw * 0.6 } else { yaw },
                            0.16 - push * 0.20,
                            if feeling {
                                frame.animate(&spin)
                            } else {
                                -0.15 + entry * 0.05 + push * 0.13
                            },
                        ],
                        0.,
                    );
                    Exposure {
                        object,
                        title_opacity: 1.,
                    }
                })
                .collect()
        };
        let mut heroes = Self {
            code: build(false),
            feeling: build(true),
            motion: motion_exposures(),
        };
        for ((exposures, keep_spinning), schedule) in [
            (&mut heroes.code, false),
            (&mut heroes.motion, false),
            (&mut heroes.feeling, true),
        ]
        .into_iter()
        .zip(objects::EXCURSIONS)
        {
            let mut poses = exposures
                .iter()
                .map(|exposure| exposure.object.clone())
                .collect::<Vec<_>>();
            objects::ring_excursion(objects, &mut poses, schedule, keep_spinning);
            let arrival = (schedule.arrival - schedule.start) as f32 / 24.;
            let exit = (schedule.return_start - schedule.start) as f32 / 24.;
            let caption = fframes::timeline!(
                at (arrival - 2. / 24.) => arrival, animate 0.0_f32 => 1.0, Easing::EaseOut,
                at exit => (exit + 3. / 24.), animate 1.0_f32 => 0.0, Easing::EaseInOut,
            );
            for (index, (exposure, object)) in exposures.iter_mut().zip(poses).enumerate() {
                exposure.object = object;
                exposure.title_opacity =
                    Frame::new(index, schedule.start + index, 24).animate(&caption);
            }
        }
        heroes
    }

    pub(super) fn draw(
        &self,
        frame: &Frame,
        ctx: &FFramesContext<'_, '_>,
        s: &Studio,
        card: Card,
    ) -> Svgr<'static> {
        let exposures = match card {
            Card::Code => &self.code,
            Card::Motion => &self.motion,
            Card::Feeling => &self.feeling,
        };
        let pose = &exposures[frame.index.min(exposures.len() - 1)];
        fframes::svgr!(<g>
            {s.background(frame,1.,0.)}
            {s.ink.draw(frame, ctx)}
            // The other eleven tracks keep rotating off-screen. This is the
            // selected track's same global pose, including both flights.
            {pose.object.draw(frame,ctx,&s.object)}
            <g opacity={pose.title_opacity}>{type_block(frame,card)}</g>
        </g>)
    }
}

fn type_block(frame: &Frame, card: Card) -> Svgr<'static> {
    let (copy, cursor) = match card {
        Card::Code => ("code.", 1120.),
        Card::Motion => ("motion.", 1256.),
        Card::Feeling => ("feeling.", 1246.),
    };
    fframes::svgr!(<g>
        <g transform={frame.animate(fframes::timeline!(
            at 0.0 => 0.166_667, animate Transform::translate(48,18) => Transform::translate(0,0), Easing::CubicBezier(0.12,1.,0.25,1.),
        ))}>
            {label(copy,800.,559.,112.,"#f4efe4",false)}
            <rect x={cursor} y="477" height="86" fill="#ed3d27"
                width={frame.animate(fframes::timeline!(at 0.0 => 0.125, animate 76.0_f32 => 6.0, Easing::EaseOut))}/>
        </g>
        <g transform={frame.animate(fframes::timeline!(
            at 0.041_667 => 0.25, animate Transform::translate(0,24) => Transform::translate(0,0), Easing::CubicBezier(0.16,1.,0.3,1.),
        ))} opacity={frame.animate(fframes::timeline!(at 0.041_667 => 0.166_667, animate 0.0_f32 => 1.0, Easing::EaseOut))}>
            {match card {
                Card::Feeling => fframes::svgr!(<g>
                    <text x="807" y="617" font-family="Instrument Serif" font-style="italic" font-size="35" fill="#d4cabb">"every frame matters."</text>
                    <path d="M811 653 C905 641 970 662 1045 647" fill="none" stroke="#ed3d27" stroke-width="2"/>
                </g>),
                Card::Code => fframes::svgr!(<text x="807" y="611" font-family="JetBrains Mono" font-size="23" fill="#bce788">"Rust + SVG"</text>),
                Card::Motion => fframes::svgr!(<text x="807" y="611" font-family="JetBrains Mono" font-size="23" fill="#c0a1ef">"timeline!"</text>),
            }}
        </g>
    </g>)
}

fn motion_exposures() -> Vec<Exposure> {
    let arrive = fframes::timeline!(
        at 0.0 => 0.166_667, animate 0.0_f32 => 1.0, Easing::CubicBezier(0.12,0.9,0.22,1.),
    );
    // Keep the measured silhouettes, but interpolate between them instead of
    // holding each exposure and jumping to the next. Start after the shared
    // ring arrival (local frame 5); the return then restores the normal aspect.
    let normal = restoration_capture::CAMERA_POSE[5];
    let tall = restoration_capture::CAMERA_POSE[12];
    let flat = restoration_capture::CAMERA_POSE[17];
    let deformation = std::array::from_fn::<_, 3, _>(|axis| {
        fframes::timeline!(
            at (5.0 / 24.0) => (10.0 / 24.0), animate normal[axis] => tall[axis], Easing::CubicBezier(0.42,0.,0.58,1.),
            at (10.0 / 24.0) => (15.0 / 24.0), animate tall[axis] => flat[axis], Easing::CubicBezier(0.42,0.,0.58,1.),
        )
    });
    (0..16)
        .map(|index| {
            let frame = Frame::new(index, index, 24);
            let entry = frame.animate(&arrive);
            let [sx, sy, roll] = deformation.each_ref().map(|curve| frame.animate(curve));
            let stretch = [sx, sy];
            let wide = ((stretch[0] - 1.) / 1.2).clamp(0., 1.);
            let object = objects::Object::new(
                5,
                [
                    -296. + wide * 25. - (1. - entry) * 60.,
                    -8. + (1. - entry) * 74.,
                    1250.,
                ],
                470. * (0.83 + entry * 0.17),
                [0.10, 0.035, roll],
                0.,
            )
            .with_stretch(stretch);
            Exposure {
                object,
                title_opacity: 1.,
            }
        })
        .collect()
}
