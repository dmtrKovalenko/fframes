//! 8.1–11.1 s: a girl (stock footage as a thermal field) dashes across and the
//! line appears in her wake. She leaves echoes; once she is gone they snap into
//! identical copies, the kind of repetition people notice instantly.

use std::sync::OnceLock;

use fframes::{FFramesContext, Frame, Svgr};

use crate::ink::{self, CREAM, ORANGE, RED, Stroke};
use crate::{GIRL, HeatDraw, Palette, Studio, copy, measure, memo, mono};

const SCALE: f32 = 0.85;
const GROUND: f32 = 1036.;
const RUN: usize = 42;
const L1: (f32, f32) = (150., 330.);
const L2: (f32, f32) = (150., 448.);
const SIZE: f32 = 104.;
const SNAP: f32 = 44.;
const SAME_POSE: usize = 18;

// Girl's centroid inside the atlas cell, measured every 6 frames by tools/prep.py.
const CX: [f32; 8] = [474., 470., 459., 429., 384., 325., 239., 190.];

fn centroid(i: usize) -> f32 {
    let f = i as f32 / 6.;
    let a = (f.floor() as usize).min(CX.len() - 2);
    let t = f - a as f32;
    CX[a] + (CX[a + 1] - CX[a]) * t.min(1.5)
}

/// Screen x of her centroid: a straight dash from off the left edge to off the right.
fn path(f: f32) -> f32 {
    150. + f * 36.
}

fn cell_x(f: usize) -> f32 {
    path(f as f32) - centroid(f) * SCALE
}

#[derive(Debug)]
pub(crate) struct Patterns {
    widths: OnceLock<(f32, f32, f32)>,
}

impl Patterns {
    pub(crate) fn new() -> Self {
        Self {
            widths: OnceLock::new(),
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
        let e = ink::exposure(frame.global_index);
        let (repeating, patterns, spot) = memo(&self.widths, || {
            (
                measure(&mut frame, ctx, "repeating ", SIZE),
                measure(&mut frame, ctx, "patterns.", SIZE),
                measure(&mut frame, ctx, "humans spot", SIZE),
            )
        });
        let top = GROUND - GIRL.cell[1] * SCALE;
        let girl_x = path(nf);
        let girl = if n < RUN {
            s.heat(
                &frame,
                ctx,
                GIRL,
                HeatDraw {
                    index: n,
                    palette: Palette::Thermal,
                    x: cell_x(n),
                    y: top,
                    scale: SCALE,
                    gain: 1.05,
                    glow: 0.5,
                    opacity: 1.,
                    hot: 0.,
                    hot_pos: [0., 0.],
                },
            )
        } else {
            Svgr::empty()
        };

        // Echo stamps every 6 frames; after she leaves they snap into one repeated pose.
        let snap = ((nf - SNAP) / 6.).clamp(0., 1.);
        let snap_e = 1. - (1. - snap).powi(3);
        let stamps = (1..=6)
            .filter_map(|k| {
                let born = k * 6;
                if n < born {
                    return None;
                }
                let free_x = cell_x(born);
                let target = 40. + (k - 1) as f32 * 214. - centroid(SAME_POSE) * SCALE + 90.;
                let x = free_x + (target - free_x) * snap_e;
                let index = if snap > 0. { SAME_POSE } else { born };
                let age = (nf - born as f32) / 24.;
                let o = if snap > 0. { 0.42 + 0.3 * snap } else { 0.6 - age * 0.25 };
                let label_x = x + centroid(index) * SCALE - 22.;
                let label = if label_x > 1380. {
                    String::new()
                } else if snap > 0.5 {
                    format!("f{SAME_POSE:02}")
                } else {
                    format!("f{born:02}")
                };
                Some(fframes::svgr!(<g>
                    {s.heat(&frame, ctx, GIRL, HeatDraw {
                        index,
                        palette: Palette::Echo,
                        x,
                        y: top,
                        scale: SCALE,
                        gain: 1.,
                        glow: 0.,
                        opacity: o,
                        hot: 0.,
                        hot_pos: [0., 0.],
                    })}
                    {mono(&label, label_x, top - 8., 15., CREAM, 0.55)}
                    <path d={format!("M{:.1} {:.1} v10", label_x + 18., top - 2.)} stroke={CREAM} stroke-width="1.5" opacity="0.4" />
                </g>))
            })
            .collect::<Vec<_>>();

        // Pen: a brace over the identical copies and a tally tick on each head.
        let row_x0 = 60.;
        let row_x1 = 40. + 5. * 214. + 150.;
        let brace_y = top - 46.;
        let mid = f32::midpoint(row_x0, row_x1);
        let brace = Stroke::new(
            &[
                [row_x0, brace_y + 22.],
                [row_x0 + 20., brace_y + 2.],
                [mid - 30., brace_y],
                [mid, brace_y - 22.],
                [mid + 30., brace_y],
                [row_x1 - 20., brace_y + 2.],
                [row_x1, brace_y + 22.],
            ],
            7.,
            CREAM,
            520.,
        )
        .draw(ink::reveal(nf, 50., 6.), e);
        let ticks = (0..6)
            .map(|k| {
                let x = 40. + k as f32 * 214. + 90. + centroid(SAME_POSE) * 0.;
                let y = top - 14.;
                Stroke::new(
                    &[[x - 2., y - 14.], [x + 3., y + 12.]],
                    6.,
                    CREAM,
                    530. + k as f32,
                )
                .draw(ink::reveal(nf, 52. + k as f32 * 1.2, 2.), e)
            })
            .collect::<Vec<_>>();
        let equals = fframes::svgr!(<g>{brace}{ticks}{mono("x6", mid - 14., brace_y - 34., 18., ORANGE, if nf > 56. { 0.9 } else { 0. })}</g>);
        let pat_c = [L2.0 + repeating + patterns / 2., L2.1 - 34.];
        let ring = Stroke::new(
            &ink::encircle(pat_c[0], pat_c[1], patterns / 2. + 34., 66., 51.),
            7.,
            RED,
            51.,
        )
        .boil(2.)
        .draw(ink::reveal(nf, 52., 8.), e);
        let tick = Stroke::new(
            &ink::underline(L1.0 + spot - 150., L1.0 + spot, L1.1 + 20., 57.),
            5.,
            ORANGE,
            57.,
        )
        .draw(ink::reveal(nf, 30., 5.), e);

        // Embers and footfalls behind her.
        let embers = (0..40)
            .filter_map(|i| {
                let k = i as f32 * 1.7;
                let born = ink::hash(k) * RUN as f32;
                let a = (nf - born) / 24.;
                if !(0. ..0.9).contains(&a) {
                    return None;
                }
                let x = path(born) - 40. - ink::hash(k + 1.) * 60. - a * 80.;
                let y = top + 120. + ink::hash(k + 2.) * 300. - a * (60. + ink::hash(k + 3.) * 120.);
                let color = ["#ffd23f", "#fb6a22", "#ff3b1f"][i % 3];
                Some(fframes::svgr!(<circle cx={x} cy={y} r={1.5 + ink::hash(k + 4.) * 2.5} fill={color} opacity={(1. - a / 0.9) * 0.9} />))
            })
            .collect::<Vec<_>>();
        let speed = if n < RUN {
            (0..5)
                .map(|k| {
                    let kk = k as f32;
                    let y = top + 140. + kk * 70. + (ink::hash(kk + nf * 0.01) - 0.5) * 10.;
                    let len = 120. + ink::hash(kk + 3.) * 160.;
                    let x1 = girl_x - 120. - ink::hash(kk + 5.) * 40.;
                    Stroke::new(
                        &[[x1 - len, y], [x1 - len * 0.4, y - 3.], [x1, y]],
                        3. + (k % 2) as f32 * 2.,
                        CREAM,
                        560. + kk,
                    )
                    .taper(0.6, 0.1)
                    .draw(1., e)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let puffs = (0..9)
            .filter_map(|k| {
                let at = 2. + k as f32 * 4.6;
                let a = (nf - at) / 24.;
                if !(0. ..0.6).contains(&a) {
                    return None;
                }
                let x = path(at) + if k % 2 == 0 { 10. } else { -30. };
                let rings = (0..3)
                    .map(|j| {
                        let jj = j as f32;
                        let r = 6. + a * (40. + jj * 25.);
                        fframes::svgr!(<circle cx={x - 20. + jj * 18. - a * 60.} cy={GROUND - 4. - a * 30. * (jj + 1.) * 0.5} r={r}
                            fill="none" stroke={CREAM} stroke-width="1.5" opacity={(1. - a / 0.6) * 0.5} />)
                    })
                    .collect::<Vec<_>>();
                Some(fframes::svgr!(<g>{rings}</g>))
            })
            .collect::<Vec<_>>();
        let steps = (0..9)
            .filter_map(|k| {
                let at = 2. + k as f32 * 4.6;
                if nf < at {
                    return None;
                }
                let x = path(at) + if k % 2 == 0 { 10. } else { -30. };
                let fade = (1. - (nf - at) / 30.).max(0.25);
                Some(fframes::svgr!(<path d={format!("M{:.1} {:.1} h{:.1}", x - 16., GROUND + 8., 26. + ink::hash(k as f32) * 20.)}
                    stroke={ORANGE} stroke-width="5" stroke-linecap="round" opacity={fade} />))
            })
            .collect::<Vec<_>>();
        let ruler = (0..13)
            .map(|k| {
                let x = 60. + k as f32 * 110.;
                let h = if k % 2 == 0 { 14. } else { 7. };
                fframes::svgr!(<path d={format!("M{x:.1} {GROUND:.1} v{h:.1}")} stroke={CREAM} stroke-width="1.5" opacity="0.35" />)
            })
            .collect::<Vec<_>>();

        let reveal = if n < RUN {
            (girl_x + 40.).max(0.)
        } else {
            1440.
        };
        fframes::svgr!(<g>
            <defs>
                <clipPath id="patterns-reveal"><rect x="0" y="0" width={reveal.max(1.)} height="1080" /></clipPath>
            </defs>
            {s.paper(&frame, 1., 0.)}
            <path d={format!("M40 {GROUND:.1} H1400")} stroke={CREAM} stroke-width="1.5" stroke-dasharray="10 12" opacity="0.35" />
            {ruler}
            <g clip-path="url(#patterns-reveal)">
                {copy("humans spot", L1.0, L1.1, SIZE, CREAM)}
                {copy("repeating patterns.", L2.0, L2.1, SIZE, CREAM)}
            </g>
            {tick}
            {ring}
            {stamps}
            {equals}
            {steps}
            {puffs}
            {speed}
            {embers}
            {girl}
        </g>)
    }
}
