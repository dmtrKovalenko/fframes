//! Reference-derived letter flights.
use fframes::{Frame, animation::Easing};

use crate::ink_capture;

#[derive(Debug, Clone, Copy)]
pub(super) struct Letter {
    pub position: [f32; 2],
    pub angle: f32,
    pub scale: f32,
}

#[derive(Debug)]
pub(super) struct Flight {
    pub letters: Vec<[Letter; 7]>,
}

impl Flight {
    pub(super) fn new() -> Self {
        // Three frames to assemble, four to pull the camera back. This is
        // a cut-speed move, not a UI-like 700 ms ease-in/out.
        let gather = fframes::timeline!(
            at 2.541_667 => 2.666_667, animate 0.0_f32 => 1.0, Easing::EaseIn,
        );
        let target = [-229., -181., -124., -47.5, 66., 167.5, 226.];
        let mut letters = Vec::with_capacity(76);
        let path = |index: f32, i: usize| {
            // Replay the measured high-energy flight in independent phases.
            // The three extra brand letters use reflected versions of those
            // trajectories, rather than orbiting seven fixed anchor points.
            let phase = (index * (0.80 + i as f32 * 0.025) + i as f32 * 3.1).rem_euclid(52.);
            let phase = if phase <= 26. { phase } else { 52. - phase };
            let sample = 3. + phase;
            let a = sample.floor() as usize;
            let b = (a + 1).min(ink_capture::LETTERS.len() - 1);
            let f = sample.fract();
            let aa = ink_capture::LETTERS[a][i % 4];
            let bb = ink_capture::LETTERS[b][i % 4];
            let mut x = aa[0] + (bb[0] - aa[0]) * f;
            let mut y = aa[1] + (bb[1] - aa[1]) * f;
            if i >= 4 {
                x = 1440. - x;
                y = 1080. - y;
            }
            // Softly fit the few off-canvas source excursions into this edit.
            [
                60. + x.clamp(0., 1440.) * (1320. / 1440.),
                85. + y.clamp(0., 1080.) * (910. / 1080.),
            ]
        };
        let mut angles = [0.0_f32; 7];
        for index in 0..76 {
            let frame = Frame::new(index, index + 413, 24);
            let g = frame.animate(&gather);
            let poses = std::array::from_fn(|i| {
                let [x, y] = path(index as f32, i);
                let [px, py] = path(index as f32 - 0.5, i);
                let speed = ((x - px).powi(2) + (y - py).powi(2)).sqrt() * 2.;
                let direction = if i % 2 == 0 { 1. } else { -1. };
                angles[i] += direction * (speed * 0.34 + 3.1);
                let margin = 52. + 270. * 4. * g * (1. - g);
                Letter {
                    position: [
                        (x * (1. - g) + (720. + target[i] * 2.8) * g).clamp(margin, 1440. - margin),
                        y * (1. - g) + (540. + 39. * 2.8) * g,
                    ],
                    angle: angles[i] * (1. - g),
                    scale: (1.05 + (speed / 170.).min(0.65)) * (1. - g) + (194. / 54. * 2.8) * g,
                }
            });
            letters.push(poses);
        }
        Self { letters }
    }
}
