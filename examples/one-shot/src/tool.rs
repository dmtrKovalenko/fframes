//! 14.1–16.1 s: the music re-drops; a vortex of pen strokes circles "AI is a tool."
//! and gets sucked towards where the author will stand.

use std::sync::OnceLock;

use fframes::{FFramesContext, Frame, Svgr};

use crate::ink::{self, BLACK, BROWN, ORANGE, RED, Stroke};
use crate::{INK_TEXT, Studio, copy, layout, memo};

const SIZE: f32 = 132.;
const Y: f32 = 590.;
const WORDS: [&str; 4] = ["AI", "is", "a", "tool."];

#[derive(Debug)]
pub(crate) struct Tool {
    layout: OnceLock<Vec<(f32, f32)>>,
}

impl Tool {
    pub(crate) fn new() -> Self {
        Self {
            layout: OnceLock::new(),
        }
    }

    pub(crate) fn render<'a>(
        &self,
        mut frame: Frame,
        ctx: &FFramesContext<'a, '_>,
        s: &Studio,
    ) -> Svgr<'a> {
        let n = frame.index;
        let nf = n as f32;
        let t = frame.seconds();
        let e = ink::exposure(frame.global_index);
        let words = memo(&self.layout, || layout(&mut frame, ctx, &WORDS, SIZE));
        let width = words[3].0 + words[3].1;
        let x0 = 720. - width / 2.;

        // The vortex contracts into the spot where the next scene's figure stands.
        let suck = ink::smoothstep(36., 48., nf);
        let center = [720. + 300. * suck, 540. + 60. * suck];
        let vortex = (0..22)
            .map(|i| {
                let k = i as f32;
                let rx = (500. + k * 13. + ink::hash(k) * 60.) * (1. - 0.85 * suck);
                let ry = (230. + k * 8. + ink::hash(k + 1.) * 40.) * (1. - 0.85 * suck);
                let speed = 2.6 + ink::hash(k + 2.) * 2.2 + suck * 6.;
                let a0 = ink::hash(k + 3.) * std::f32::consts::TAU + t * speed;
                let len = 0.6 + ink::hash(k + 4.) * 1.1;
                let tilt: f32 = -0.18;
                let pts: Vec<[f32; 2]> = ink::arc(0., 0., rx, ry, a0, a0 + len, 600. + k)
                    .into_iter()
                    .map(|[x, y]| {
                        [
                            center[0] + x * tilt.cos() - y * tilt.sin(),
                            center[1] + x * tilt.sin() + y * tilt.cos(),
                        ]
                    })
                    .collect();
                let color = [BROWN, BLACK, RED, ORANGE, BROWN, BLACK][i % 6];
                let w = 3. + ink::hash(k + 5.).powi(2) * 13.;
                Stroke::new(&pts, w, color, 600. + k)
                    .taper(0.05, 0.6)
                    .boil(1.5)
                    .draw(0.55 + 0.45 * ink::reveal(nf, k * 0.25, 5.), e)
            })
            .collect::<Vec<_>>();
        let flung = (0..36)
            .filter_map(|i| {
                let k = i as f32 * 1.3;
                let born = ink::hash(k) * 40.;
                let a = (nf - born) / 24.;
                if !(0. ..0.6).contains(&a) {
                    return None;
                }
                let ang = ink::hash(k + 1.) * std::f32::consts::TAU;
                let r0 = 380. + ink::hash(k + 2.) * 200.;
                let v = 300. + ink::hash(k + 3.) * 500.;
                let x = center[0] + ang.cos() * (r0 * 1.3 + v * a);
                let y = center[1] + ang.sin() * (r0 * 0.6 + v * a * 0.6);
                let color = [BLACK, RED, BROWN][i % 3];
                Some(fframes::svgr!(<circle cx={x} cy={y} r={(2. + ink::hash(k + 4.) * 5.) * (1. - a / 0.6)} fill={color} />))
            })
            .collect::<Vec<_>>();

        let text = WORDS
            .iter()
            .zip(&words)
            .enumerate()
            .map(|(i, (w, &(x, _)))| {
                let at = i as f32 * 2. - 2.;
                let a = ((nf - at) / 4.).clamp(0., 1.);
                if a <= 0. {
                    return Svgr::empty();
                }
                let rise = (1. - a).powi(2) * 40.;
                let color = if i == 3 { ORANGE } else { INK_TEXT };
                fframes::svgr!(<g transform={format!("translate(0 {rise:.1})")} opacity={a}>{copy(w, x0 + x, Y, SIZE, color)}</g>)
            })
            .collect::<Vec<_>>();
        let tool = words[3];
        let under = Stroke::new(
            &ink::underline(x0 + tool.0, x0 + tool.0 + tool.1 - 20., Y + 30., 81.),
            9.,
            BLACK,
            81.,
        )
        .draw(ink::reveal(nf, 12., 5.), e);
        let ai = words[0];
        let arrow_pts = [
            [x0 + ai.0 + ai.1 / 2., Y - SIZE * 0.85],
            [x0 + ai.0 + ai.1 * 1.5, Y - SIZE * 1.45],
            [x0 + tool.0 + tool.1 * 0.25, Y - SIZE * 1.4],
            [x0 + tool.0 + tool.1 * 0.4, Y - SIZE * 0.95],
        ];
        let arrow = Stroke::new(&arrow_pts, 5., RED, 83.).draw(ink::reveal(nf, 16., 6.), e);
        let head_tip = arrow_pts[3];
        let head = Stroke::new(
            &[
                [head_tip[0] - 26., head_tip[1] - 30.],
                head_tip,
                [head_tip[0] + 22., head_tip[1] - 34.],
            ],
            5.,
            RED,
            85.,
        )
        .draw(ink::reveal(nf, 21., 3.), e);
        let fade = 1. - suck * 0.85;

        fframes::svgr!(<g>
            {s.paper(&frame, 0., 0.15)}
            {vortex}
            {flung}
            <g opacity={fade}>
                {text}
                {under}
                {arrow}
                {head}
            </g>
        </g>)
    }
}
