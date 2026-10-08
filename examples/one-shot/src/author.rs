//! 16.1–19.1 s: an anonymous figure (stock footage reduced to a yellow heat
//! silhouette) stands up and rises into the frame from its bottom edge, measured by
//! a pen ruler: "you are the author."

use std::sync::OnceLock;

use fframes::{FFramesContext, Frame, Svgr};

use crate::ink::{self, CREAM, ORANGE, Stroke};
use crate::{HeatDraw, Palette, RISE, Studio, copy, layout, memo, mono};

const SIZE: f32 = 108.;
const X: f32 = 104.;
const Y1: f32 = 476.;
const Y2: f32 = 598.;
const SCALE: f32 = 2.23;
// Head centre in the atlas cell, and where it ends up on screen once standing.
const HEAD_CELL: [f32; 2] = [172., 25.];
const HEAD_X: f32 = 1010.;
const STAND_TOP: f32 = 250.;
const RULER_X: f32 = 1352.;

type Words = Vec<(f32, f32)>;

#[derive(Debug)]
pub(crate) struct Author {
    layout: OnceLock<(Words, Words)>,
}

/// 0 → 1 with a small overshoot, like something pushed up from below.
fn lift(t: f32) -> f32 {
    if t <= 0. {
        return 0.;
    }
    1. - (-t * 5.).exp() * (t * 6.).cos()
}

impl Author {
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
        let (l1, l2) = memo(&self.layout, || {
            (
                layout(&mut frame, ctx, &["you", "are"], SIZE),
                layout(&mut frame, ctx, &["the", "author."], SIZE),
            )
        });

        // The footage stands up on its own; the whole figure is also pushed up from
        // below the frame so it reveals itself from the bottom edge.
        let index = ((nf * 1.25) as usize).min(60);
        let push = 1. - lift(t - 0.12);
        let extra = 560. * push;
        let warm = ink::smoothstep(0.05, 0.9, t);
        let x0 = HEAD_X - HEAD_CELL[0] * SCALE;
        let y0 = STAND_TOP - HEAD_CELL[1] * SCALE + extra;
        let figure = s.heat(
            &frame,
            ctx,
            RISE,
            HeatDraw {
                index,
                palette: Palette::Yellow,
                x: x0,
                y: y0,
                scale: SCALE,
                gain: 0.75 + 0.3 * warm,
                glow: 0.4 + 0.5 * warm,
                opacity: 1.,
                hot: 0.,
                hot_pos: [0., 0.],
            },
        );
        // Head top in the cell moves from ~145 (seated) to ~25 (standing).
        let seated = 1. - ink::smoothstep(8., 44., nf);
        let head_top = y0 + (25. + 120. * seated) * SCALE;
        let head = [HEAD_X, head_top + 80.];

        let rays = (0..18)
            .map(|i| {
                let k = i as f32;
                let ang = k / 18. * std::f32::consts::TAU + t * 0.1;
                let spread = 0.04 + ink::hash(k) * 0.05;
                let r = 1300.;
                let d = format!(
                    "M{:.1} {:.1} L{:.1} {:.1} L{:.1} {:.1}Z",
                    head[0],
                    head[1],
                    head[0] + (ang - spread).cos() * r,
                    head[1] + (ang - spread).sin() * r,
                    head[0] + (ang + spread).cos() * r,
                    head[1] + (ang + spread).sin() * r
                );
                fframes::svgr!(<path d={d} fill="#f7c51e" opacity={(0.02 + ink::hash(k + 1.) * 0.03) * warm} />)
            })
            .collect::<Vec<_>>();
        // Heat pours up from the bottom edge where the figure comes from.
        let embers = (0..64)
            .map(|i| {
                let k = i as f32 * 1.9;
                let period = 0.8 + ink::hash(k) * 1.2;
                let p = ((t + ink::hash(k + 1.) * period) % period) / period;
                let x = 640. + ink::hash(k + 2.) * 760. + (p * 6. + k).sin() * 22.;
                let y = 1100. - p * (380. + ink::hash(k + 3.) * 420.);
                let o = (1. - p) * (0.3 + 0.7 * warm) * (0.6 + 0.4 * (t * 20. + k).sin());
                let color = ["#ffe66d", "#ffb703", "#fb6a22"][i % 3];
                fframes::svgr!(<circle cx={x} cy={y} r={1.2 + ink::hash(k + 4.) * 2.8} fill={color} opacity={o} />)
            })
            .collect::<Vec<_>>();
        // The last of the vortex spirals into the bottom edge and fades.
        let orbit = (0..9)
            .map(|i| {
                let k = i as f32;
                let fade = 1. - ink::smoothstep(2., 18., nf);
                if fade <= 0. {
                    return Svgr::empty();
                }
                let r = (220. - nf * 6. + k * 14.).max(20.);
                let a0 = k * 0.7 + t * (6. - k * 0.3);
                let pts = ink::arc(HEAD_X, 1060., r * 1.3, r * 0.4, a0, a0 + 0.9, 700. + k);
                let color = [CREAM, ORANGE, "#f7c51e"][i % 3];
                fframes::svgr!(<g opacity={fade}>{Stroke::new(&pts, 3. + (i % 3) as f32 * 2., color, 700. + k).taper(0.05, 0.6).draw(1., e)}</g>)
            })
            .collect::<Vec<_>>();

        // A pen ruler on the right measures the rise.
        let ticks = (0..24)
            .map(|i| {
                let y = 1060. - i as f32 * 40.;
                let long = i % 5 == 0;
                fframes::svgr!(<path d={format!("M{RULER_X:.1} {y:.1} h{}", if long { 26 } else { 12 })} stroke={CREAM} stroke-width="1.5" opacity={0.45 * warm} />)
            })
            .collect::<Vec<_>>();
        let marker_y = head_top.clamp(120., 1060.);
        let height = ((1080. - marker_y) / 4.6) as i32 + 40;
        let marker = fframes::svgr!(<g opacity={warm}>
            <path d={format!("M{:.1} {marker_y:.1} H{:.1}", RULER_X - 8., HEAD_X + 120.)} stroke={ORANGE} stroke-width="2" stroke-dasharray="6 6" />
            <path d={format!("M{:.1} {:.1} l14 -8 v16 Z", RULER_X - 6., marker_y)} fill={ORANGE} />
            {mono(&format!("h {height:03}"), RULER_X - 72., marker_y - 14., 14., CREAM, 0.85)}
        </g>);
        let ruler_ink = Stroke::new(
            &[[RULER_X, 1070.], [RULER_X + 2., 600.], [RULER_X - 1., 120.]],
            3.,
            CREAM,
            760.,
        )
        .draw(ink::reveal(nf, 2., 10.), e);

        let word = |text: &'static str, x: f32, y: f32, at: f32, color: &str| {
            let a = ((nf - at) / 4.).clamp(0., 1.);
            if a <= 0. {
                return Svgr::empty();
            }
            let rise = (1. - a).powi(3) * 60.;
            fframes::svgr!(<g transform={format!("translate(0 {rise:.1})")} opacity={a}>{copy(text, x, y, SIZE, color)}</g>)
        };
        let you = l1[0];
        let author = l2[1];
        let ring = Stroke::new(
            &ink::encircle(X + you.0 + you.1 / 2., Y1 - 36., you.1 / 2. + 16., 62., 91.),
            8.,
            ORANGE,
            91.,
        )
        .boil(2.)
        .draw(ink::reveal(nf, 26., 8.), e);
        let under = Stroke::new(
            &ink::underline(X + author.0, X + author.0 + author.1, Y2 + 26., 93.),
            6.,
            CREAM,
            93.,
        )
        .draw(ink::reveal(nf, 40., 6.), e);
        let under2 = Stroke::new(
            &ink::underline(
                X + author.0 + 20.,
                X + author.0 + author.1 - 30.,
                Y2 + 46.,
                95.,
            ),
            4.,
            CREAM,
            95.,
        )
        .draw(ink::reveal(nf, 45., 6.), e);
        let sparks = (0..6)
            .map(|i| {
                let k = i as f32;
                let at = 30. + k * 3.;
                let a = ((nf - at) / 4.).clamp(0., 1.);
                let x = X + you.0 + you.1 / 2. + (k * 2.4).cos() * (you.1 / 2. + 60.);
                let y = Y1 - 36. + (k * 2.4).sin() * 92.;
                let tw = 0.8 + 0.2 * (t * 7. + k).sin();
                ink::sparkle(
                    x,
                    y,
                    (7. + ink::hash(k) * 9.) * a * tw,
                    if i % 2 == 0 { ORANGE } else { CREAM },
                )
            })
            .collect::<Vec<_>>();

        fframes::svgr!(<g>
            <defs>
                <radialGradient id="author-halo">
                    <stop offset="0" stop-color="#ffd23f" stop-opacity="0.34" />
                    <stop offset="1" stop-color="#ffd23f" stop-opacity="0" />
                </radialGradient>
                <radialGradient id="author-floor">
                    <stop offset="0" stop-color="#ffb703" stop-opacity="0.4" />
                    <stop offset="1" stop-color="#ffb703" stop-opacity="0" />
                </radialGradient>
            </defs>
            {s.paper(&frame, 1., 0.)}
            {rays}
            <ellipse cx={head[0]} cy={head[1] + 120.} rx={380. + 160. * warm} ry={420. + 160. * warm} fill="url(#author-halo)" opacity={warm} />
            <ellipse cx={HEAD_X} cy="1090" rx="620" ry="220" fill="url(#author-floor)" opacity={0.4 + 0.6 * (1. - push)} />
            {orbit}
            {figure}
            {embers}
            {ruler_ink}
            {ticks}
            {marker}
            {word("you", X + l1[0].0, Y1, 8., ORANGE)}
            {word("are", X + l1[1].0, Y1, 11., CREAM)}
            {word("the", X + l2[0].0, Y2, 17., CREAM)}
            {word("author.", X + l2[1].0, Y2, 20., CREAM)}
            {ring}
            {sparks}
            {under}
            {under2}
        </g>)
    }
}
