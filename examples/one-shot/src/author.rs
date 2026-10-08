//! 16.1–19.1 s: a man in profile (stock footage as a yellow heat field) rises into
//! the frame out of a cold blur, lifts his hand and reaches for the words:
//! "you are the author."

use std::sync::OnceLock;

use fframes::{FFramesContext, Frame, Svgr};

use crate::ink::{self, CREAM, ORANGE, Stroke};
use crate::{HeatDraw, Palette, REACH, Studio, copy, layout, memo};

const SIZE: f32 = 108.;
const X: f32 = 104.;
const Y1: f32 = 214.;
const Y2: f32 = 336.;
const SCALE: f32 = 1440. / 1126.;
const HEAD: [f32; 2] = [1060., 470.];
// Where the open palm ends up once the arm is out (screen space, after the rise).
const PALM: [f32; 2] = [40., 530.];
const REACHED: f32 = 34.;

type Words = Vec<(f32, f32)>;

#[derive(Debug)]
pub(crate) struct Author {
    layout: OnceLock<(Words, Words)>,
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

        // Rise into frame from below, out of focus and cold, then warm up.
        let a = t.max(0.);
        let rise = 260. * (-a * 6.).exp() * (a * 7.).cos().max(-0.3);
        let warm = ink::smoothstep(0., 0.6, t);
        let focus = (1. - ink::smoothstep(0., 0.35, t)) * 26.;
        let index = ((nf * 60. / 64.) as usize).min(REACH.frames - 1);
        let top = 1080. - REACH.cell[1] * SCALE;
        let figure = s.heat(
            &frame,
            ctx,
            REACH,
            HeatDraw {
                index,
                palette: if t < 0.12 {
                    Palette::Echo
                } else {
                    Palette::Yellow
                },
                x: 0.,
                y: top,
                scale: SCALE,
                gain: 0.6 + 0.42 * warm,
                glow: 0.35 + 0.45 * warm,
                opacity: 1.,
                hot: 0.,
                hot_pos: [0., 0.],
            },
        );
        let figure = if focus > 0.5 {
            fframes::svgr!(<g filter="url(#author-focus)">{figure}</g>)
        } else {
            figure
        };

        let rays = (0..16)
            .map(|i| {
                let k = i as f32;
                let ang = k / 16. * std::f32::consts::TAU + t * 0.1;
                let spread = 0.05 + ink::hash(k) * 0.05;
                let r = 1100.;
                let (cx, cy) = (HEAD[0], HEAD[1] + rise);
                let d = format!(
                    "M{cx:.1} {cy:.1} L{:.1} {:.1} L{:.1} {:.1}Z",
                    cx + (ang - spread).cos() * r,
                    cy + (ang - spread).sin() * r,
                    cx + (ang + spread).cos() * r,
                    cy + (ang + spread).sin() * r
                );
                fframes::svgr!(<path d={d} fill="#f7c51e" opacity={(0.02 + ink::hash(k + 1.) * 0.03) * warm} />)
            })
            .collect::<Vec<_>>();
        let embers = (0..56)
            .map(|i| {
                let k = i as f32 * 1.9;
                let period = 0.9 + ink::hash(k) * 1.3;
                let p = ((t + ink::hash(k + 1.) * period) % period) / period;
                let x = 760. + ink::hash(k + 2.) * 560. + (p * 6. + k).sin() * 22.;
                let y = 1080. - ink::hash(k + 3.) * 520. - p * 300. + rise;
                let o = (1. - p) * warm * (0.6 + 0.4 * (t * 20. + k).sin());
                let color = ["#ffe66d", "#ffb703", "#fb6a22"][i % 3];
                fframes::svgr!(<circle cx={x} cy={y} r={1.2 + ink::hash(k + 4.) * 2.6} fill={color} opacity={o} />)
            })
            .collect::<Vec<_>>();
        // The last of the vortex winds around his head and fades.
        let orbit = (0..9)
            .map(|i| {
                let k = i as f32;
                let fade = 1. - ink::smoothstep(4., 22., nf);
                if fade <= 0. {
                    return Svgr::empty();
                }
                let r = 150. + k * 14. + nf * 5.;
                let a0 = k * 0.7 + t * (5. - k * 0.3);
                let pts = ink::arc(HEAD[0], HEAD[1] + rise - 40., r * 1.2, r * 0.55, a0, a0 + 0.9, 700. + k);
                let color = [CREAM, ORANGE, "#f7c51e"][i % 3];
                fframes::svgr!(<g opacity={fade}>{Stroke::new(&pts, 3. + (i % 3) as f32 * 2., color, 700. + k).taper(0.05, 0.6).draw(1., e)}</g>)
            })
            .collect::<Vec<_>>();

        let word = |text: &'static str, x: f32, y: f32, at: f32, color: &str| {
            let a = ((nf - at) / 4.).clamp(0., 1.);
            if a <= 0. {
                return Svgr::empty();
            }
            let lift = (1. - a).powi(3) * 50.;
            fframes::svgr!(<g transform={format!("translate(0 {lift:.1})")} opacity={a}>{copy(text, x, y, SIZE, color)}</g>)
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
        .draw(ink::reveal(nf, 22., 8.), e);
        let under = Stroke::new(
            &ink::underline(X + author.0, X + author.0 + author.1, Y2 + 26., 93.),
            6.,
            CREAM,
            93.,
        )
        .draw(ink::reveal(nf, REACHED + 4., 6.), e);
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
        .draw(ink::reveal(nf, REACHED + 9., 6.), e);
        let sparks = (0..6)
            .map(|i| {
                let k = i as f32;
                let at = 28. + k * 3.;
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
        // The reach: pen sparks fly off his fingertips toward the words.
        let touch = (0..7)
            .map(|i| {
                let k = i as f32;
                let ang = -1.65 + k * 0.24;
                let r0 = 40. + ink::hash(k) * 16.;
                let r1 = r0 + 40. + ink::hash(k + 1.) * 50.;
                let p0 = [PALM[0] + ang.cos() * r0, PALM[1] + ang.sin() * r0];
                let p1 = [PALM[0] + ang.cos() * r1, PALM[1] + ang.sin() * r1];
                let on = ink::reveal(nf, REACHED + k * 0.6, 3.);
                let off = ((nf - REACHED - 14.) / 6.).clamp(0., 1.);
                Stroke::new(
                    &[p0, p1],
                    4.5,
                    if i % 2 == 0 { ORANGE } else { CREAM },
                    720. + k,
                )
                .taper(0.2, 0.5)
                .draw_window(off, on, e)
            })
            .collect::<Vec<_>>();
        let touch_glow = ink::smoothstep(REACHED - 4., REACHED + 6., nf)
            * (1. - ink::smoothstep(REACHED + 20., REACHED + 30., nf));

        fframes::svgr!(<g>
            <defs>
                <radialGradient id="author-halo">
                    <stop offset="0" stop-color="#ffd23f" stop-opacity="0.34" />
                    <stop offset="1" stop-color="#ffd23f" stop-opacity="0" />
                </radialGradient>
                <filter id="author-focus" x="-10%" y="-10%" width="120%" height="120%"><feGaussianBlur stdDeviation={focus} /></filter>
            </defs>
            {s.paper(&frame, 1., 0.)}
            {rays}
            <ellipse cx={HEAD[0]} cy={HEAD[1] + rise} rx={420. + 140. * warm} ry={380. + 120. * warm} fill="url(#author-halo)" opacity={warm} />
            <ellipse cx={PALM[0] + 40.} cy={PALM[1]} rx="160" ry="160" fill="url(#author-halo)" opacity={touch_glow} />
            {orbit}
            <g transform={format!("translate(0 {rise:.1})")}>{figure}</g>
            {embers}
            {word("you", X + l1[0].0, Y1, 6., ORANGE)}
            {word("are", X + l1[1].0, Y1, 9., CREAM)}
            {word("the", X + l2[0].0, Y2, 14., CREAM)}
            {word("author.", X + l2[1].0, Y2, 17., CREAM)}
            {ring}
            {sparks}
            {touch}
            {under}
            {under2}
        </g>)
    }
}
