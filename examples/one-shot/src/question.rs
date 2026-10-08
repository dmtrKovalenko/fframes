//! 0–5.6 s, over the soundtrack's quiet break: oversized type, flash frames, the
//! question typed out with a nervous pen, then the camera falls into "stunning".

use std::sync::OnceLock;

use fframes::{FFramesContext, Frame, ShaderUniforms, Svgr, animation::Easing};

use crate::ink::{self, BROWN, CREAM, ORANGE, RED, Stroke};
use crate::{INK_TEXT, Studio, copy, layout, memo, mono};

const LINE1: [&str; 6] = ["how", "do", "you", "make", "AI", "produce"];
const LINE2: [&str; 5] = ["stunning", "results", "in", "one", "shot?"];
const SIZE: f32 = 64.;
const X0: f32 = 150.;
const Y1: f32 = 470.;
const Y2: f32 = 556.;
const TYPE_START: usize = 32;
const TYPE_EVERY: usize = 3;

#[derive(Debug, Clone)]
struct Layout {
    line1: Vec<(f32, f32)>,
    line2: Vec<(f32, f32)>,
    how: f32,
    how_do_you: f32,
    ai: f32,
    produce: f32,
}

#[derive(Debug)]
pub(crate) struct Question {
    layout: OnceLock<Layout>,
    question_mark: Vec<Stroke>,
    gestures: Vec<(usize, usize, Stroke)>,
    streaks: Vec<(f32, f32, f32, f32)>,
}

fn ease(t: f32) -> f32 {
    // CubicBezier(0.7, 0, 0.2, 1)-like: slow start, hard brake.
    let t = t.clamp(0., 1.);
    if t < 0.5 {
        4. * t * t * t
    } else {
        1. - (-2. * t + 2.).powi(3) / 2.
    }
}

impl Question {
    pub(crate) fn new() -> Self {
        // A big question mark written with a brush in four quick exposures.
        let question_mark = vec![
            Stroke::new(
                &[
                    [600., 380.],
                    [628., 300.],
                    [720., 262.],
                    [812., 300.],
                    [826., 384.],
                    [760., 448.],
                    [716., 500.],
                    [712., 590.],
                ],
                30.,
                BROWN,
                3.,
            )
            .boil(3.),
            Stroke::new(
                &[[700., 668.], [716., 660.], [724., 676.], [708., 686.]],
                34.,
                BROWN,
                5.,
            )
            .boil(2.),
        ];
        // The random pen: marks that appear around the words while they type,
        // above, below and to the right of the block, never over a word.
        let gestures = (0..26)
            .map(|i| {
                let k = i as f32;
                let zone = i % 3;
                let (x, y, w, h) = match zone {
                    0 => (
                        90. + ink::hash(k) * 760.,
                        240. + ink::hash(k + 1.) * 90.,
                        160. + ink::hash(k + 2.) * 160.,
                        60. + ink::hash(k + 3.) * 70.,
                    ),
                    1 => (
                        90. + ink::hash(k) * 820.,
                        610. + ink::hash(k + 1.) * 160.,
                        150. + ink::hash(k + 2.) * 180.,
                        50. + ink::hash(k + 3.) * 90.,
                    ),
                    _ => (
                        1060. + ink::hash(k) * 180.,
                        300. + ink::hash(k + 1.) * 420.,
                        110. + ink::hash(k + 2.) * 90.,
                        80. + ink::hash(k + 3.) * 110.,
                    ),
                };
                let ctrl = if i % 2 == 0 {
                    ink::cursive(x, y, w, h * 0.7, 11. + k * 7.3)
                } else {
                    ink::gesture(x, y, w, h, 11. + k * 7.3, 5 + i % 5)
                };
                let width = 2.5 + ink::hash(k + 5.) * 4.5;
                let color = [BROWN, RED, BROWN, crate::ink::BLACK][i % 4];
                let at = TYPE_START + 1 + i * 2 - (i / 9) * 3;
                (
                    at,
                    6 + i % 7,
                    Stroke::new(&ctrl, width, color, 40. + k).boil(1.8),
                )
            })
            .collect();
        let streaks = (0..14)
            .map(|i| {
                let k = i as f32;
                let y = if i % 2 == 0 { Y1 - 20. } else { Y2 - 20. } + (ink::hash(k) - 0.5) * 140.;
                let x = 60. + ink::hash(k + 3.) * 500.;
                let len = 400. + ink::hash(k + 5.) * 700.;
                let w = 8. + ink::hash(k + 7.) * 22.;
                (x, y, len, w)
            })
            .collect();
        Self {
            layout: OnceLock::new(),
            question_mark,
            gestures,
            streaks,
        }
    }

    fn layout(&self, frame: &mut Frame, ctx: &FFramesContext<'_, '_>) -> Layout {
        memo(&self.layout, || Layout {
            line1: layout(frame, ctx, &LINE1, SIZE),
            line2: layout(frame, ctx, &LINE2, SIZE),
            how: crate::measure(frame, ctx, "how", 340.),
            how_do_you: crate::measure(frame, ctx, "how do you", 340.),
            ai: crate::measure(frame, ctx, "AI", 520.),
            produce: crate::measure(frame, ctx, "produce", 300.),
        })
    }

    pub(crate) fn render<'a>(
        &self,
        mut frame: Frame,
        ctx: &FFramesContext<'a, '_>,
        s: &Studio,
    ) -> Svgr<'a> {
        let n = frame.index;
        let lay = self.layout(&mut frame, ctx);
        let e = ink::exposure(frame.global_index);
        let content = if n < 4 {
            self.question_mark(n, e)
        } else if n < TYPE_START {
            self.oversized(&frame, n, &lay, e)
        } else {
            self.sentence(&frame, n, &lay, e, s)
        };
        let dark = if n == 13 { 1. } else { 0. };
        let flash = if n == 4 {
            fframes::svgr!(<rect width="1440" height="1080" fill="#fb6a22" />)
        } else {
            Svgr::empty()
        };
        fframes::svgr!(<g>{s.paper(&frame, dark, 0.15)}{flash}{content}</g>)
    }

    fn question_mark(&self, n: usize, e: usize) -> Svgr<'static> {
        let r = [0.38, 0.7, 0.95, 1.][n];
        let mark = self.question_mark[0].draw(r, e);
        let dot = if n >= 2 {
            self.question_mark[1].draw(1., e)
        } else {
            Svgr::empty()
        };
        let flecks = (0..9)
            .map(|i| {
                let k = i as f32 * 3.1;
                let x = 560. + ink::hash(k) * 340.;
                let y = 250. + ink::hash(k + 1.) * 470.;
                let r = 2. + ink::hash(k + 2.) * 5.;
                if n * 3 >= i {
                    fframes::svgr!(<circle cx={x} cy={y} r={r} fill={BROWN} />)
                } else {
                    Svgr::empty()
                }
            })
            .collect::<Vec<_>>();
        fframes::svgr!(<g>{mark}{dot}{flecks}</g>)
    }

    fn rulers(y_base: f32, size: f32, color: &str, offset: f32) -> Svgr<'static> {
        let y_x = y_base - size * 0.545;
        fframes::svgr!(<path d={format!("M-40 {y_base:.1} H1480 M-40 {y_x:.1} H1480")} fill="none"
            stroke={color.to_owned()} stroke-width="4" stroke-dasharray="44 30" stroke-dashoffset={offset} opacity="0.85" />)
    }

    fn oversized(&self, frame: &Frame, n: usize, lay: &Layout, e: usize) -> Svgr<'static> {
        match n {
            4 => {
                let x = 720. - lay.how / 2.;
                fframes::svgr!(<g>
                    {Self::rulers(660., 340., "#2a1a10", 0.)}
                    {copy("how", x, 660., 340., INK_TEXT)}
                </g>)
            }
            5..=12 => {
                let t = (n - 5) as f32 / 7.;
                let pan = -ease(t) * (lay.how_do_you - 900.);
                let caret_x = 180. + lay.how_do_you + 24. + pan;
                let scribble = Stroke::new(
                    &ink::zigzag(180. + lay.how + 120. + pan, 700., 230., 26., 7, 8.),
                    6.,
                    crate::ink::RED,
                    8.,
                )
                .draw(ink::reveal(n as f32, 7., 4.), e);
                fframes::svgr!(<g>
                    {Self::rulers(660., 340., BROWN, n as f32 * 9.)}
                    <g transform={format!("translate({pan:.1} 0)")}>
                        {copy("how do you", 180., 660., 340., INK_TEXT)}
                    </g>
                    <rect x={caret_x} y="380" width="14" height="300" fill={ORANGE} opacity={if n % 4 < 2 { 1. } else { 0.2 }} />
                    {scribble}
                    {mono(&format!("x-height  {:>4}", 185), 30., 470., 14., BROWN, 0.7)}
                    {mono("baseline", 30., 690., 14., BROWN, 0.7)}
                </g>)
            }
            13 => fframes::svgr!(<g>
                {Self::rulers(660., 340., ORANGE, 0.)}
                <g transform="translate(-160 0) skewX(-6)">{copy("make AI", 60., 660., 340., CREAM)}</g>
            </g>),
            14..=22 => self.selection(frame, n, lay),
            23..=26 => {
                let t = (n - 23) as f32 / 3.;
                let x = 1500. - t * (lay.produce + 1800.);
                let lines = (0..9)
                    .map(|i| {
                        let y = 380. + i as f32 * 36. + (ink::hash(i as f32 + n as f32) - 0.5) * 20.;
                        let x0 = x + lay.produce + 40. + ink::hash(i as f32 * 2.) * 200.;
                        let len = 200. + ink::hash(i as f32 * 5. + n as f32) * 600.;
                        fframes::svgr!(<path d={format!("M{x0:.1} {y:.1} h{len:.1}")} stroke={RED} stroke-width={3. + (i % 3) as f32 * 3.} stroke-linecap="round" opacity="0.7" />)
                    })
                    .collect::<Vec<_>>();
                fframes::svgr!(<g>
                    {lines}
                    <g transform={format!("translate({x:.1} 0) skewX(-14)")}>{copy("produce", 0., 650., 300., RED)}</g>
                </g>)
            }
            _ => {
                // The big letters collapse into the two lines the sentence will use.
                let t = (n - 27) as f32 / 4.;
                let streaks = self
                    .streaks
                    .iter()
                    .enumerate()
                    .map(|(i, &(x, y, len, w))| {
                        let target = if i % 2 == 0 { Y1 - 22. } else { Y2 - 22. };
                        let yy = y + (target - y) * ease(t);
                        let ll = len * (1. - t * 0.85);
                        let xx = x + (X0 - x) * t;
                        let ww = w * (1. - t * 0.8);
                        fframes::svgr!(<path d={format!("M{xx:.1} {yy:.1} h{ll:.1}")} stroke={if i % 5 == 0 { RED } else { INK_TEXT }}
                            stroke-width={ww} stroke-linecap="round" opacity={0.85 - t * 0.4} />)
                    })
                    .collect::<Vec<_>>();
                fframes::svgr!(<g>{streaks}</g>)
            }
        }
    }

    fn selection(&self, frame: &Frame, n: usize, lay: &Layout) -> Svgr<'static> {
        let scale = frame.animate(&fframes::timeline!(
            at 0.0, animate 1.16_f32 => 1.0, Easing::Spring { mass: 1.0, stiffness: 260.0, damping: 15.0 },
        ));
        let (w, h) = (lay.ai * scale, 375. * scale);
        let (x, y) = (720. - w / 2. - 30., 560. - h / 2. - 40.);
        let (bw, bh) = (w + 60., h + 80.);
        let handles = [
            (0., 0.),
            (0.5, 0.),
            (1., 0.),
            (0., 0.5),
            (1., 0.5),
            (0., 1.),
            (0.5, 1.),
            (1., 1.),
        ]
        .iter()
        .map(|&(u, v)| {
            fframes::svgr!(<rect x={x + u * bw - 10.} y={y + v * bh - 10.} width="20" height="20"
                    fill="#efe9dc" stroke={BROWN} stroke-width="3" />)
        })
        .collect::<Vec<_>>();
        let cursor_x = x + bw + 6.;
        let cursor_y = y + bh + 6.;
        let label = format!("W {:04}  H {:04}", (bw * 1.6) as i32, (bh * 1.6) as i32);
        fframes::svgr!(<g>
            {Self::rulers(720., 520., BROWN, n as f32 * 7.)}
            <g transform={format!("translate(720 560) scale({scale:.4}) translate(-720 -560)")}>
                {copy("AI", 720. - lay.ai / 2., 748., 520., INK_TEXT)}
            </g>
            <rect x={x} y={y} width={bw} height={bh} fill="none" stroke={BROWN} stroke-width="3" stroke-dasharray="14 10" stroke-dashoffset={n as f32 * 4.} />
            <path d={format!("M{:.1} {:.1} v-64", x + bw / 2., y)} stroke={BROWN} stroke-width="3" />
            <circle cx={x + bw / 2.} cy={y - 74.} r="11" fill="#efe9dc" stroke={BROWN} stroke-width="3" />
            {handles}
            {mono(&label, x, y + bh + 44., 16., BROWN, 0.85)}
            <path d={format!("M{cursor_x:.1} {cursor_y:.1} l0 46 l12 -12 l10 22 l9 -4 l-10 -21 l17 0 Z")} fill={INK_TEXT} stroke="#efe9dc" stroke-width="2" />
        </g>)
    }

    fn sentence(
        &self,
        frame: &Frame,
        n: usize,
        lay: &Layout,
        e: usize,
        s: &Studio,
    ) -> Svgr<'static> {
        let t = frame.seconds();
        let typed = (n.saturating_sub(TYPE_START) / TYPE_EVERY + 1).min(11);
        let stunning = lay.line2[0];
        let stun_c = [X0 + stunning.0 + stunning.1 / 2., Y2 - 22.];
        let one = lay.line2[3];
        let shot = lay.line2[4];
        let one_c = [X0 + one.0 + (shot.0 + shot.1 - one.0) / 2., Y2 - 22.];

        // Camera: fall into "stunning", drift over to "one shot?", punch in.
        let z = ease(((n as f32 - 74.) / 22.).clamp(0., 1.));
        let pan = ease(((n as f32 - 110.) / 12.).clamp(0., 1.));
        let punch = ease(((n as f32 - 127.) / 8.).clamp(0., 1.));
        let focus = [
            720. + (stun_c[0] - 720.) * z + (one_c[0] - stun_c[0]) * pan,
            540. + (stun_c[1] - 540.) * z + (one_c[1] - stun_c[1]) * pan,
        ];
        let drift = (t * 1.3).sin() * 6. * z;
        let scale = 1. + 1.5 * z - 0.35 * pan + 1.6 * punch;
        let camera = format!(
            "translate({:.1} {:.1}) scale({scale:.4}) translate({:.1} {:.1})",
            720. + drift,
            540.,
            -focus[0],
            -focus[1]
        );

        let dim = 1. - 0.72 * z;
        let block = (lay.line1[5].0 + lay.line1[5].1).max(lay.line2[4].0 + lay.line2[4].1);
        let rules = fframes::svgr!(<path d={format!(
            "M{:.1} {Y1:.1} H{:.1} M{:.1} {:.1} H{:.1} M{:.1} {Y2:.1} H{:.1} M{:.1} {:.1} H{:.1}",
            X0 - 60., X0 + block + 80., X0 - 60., Y1 - SIZE * 0.545, X0 + block + 80., X0 - 60., X0 + block + 80., X0 - 60., Y2 - SIZE * 0.545, X0 + block + 80.
        )} stroke={BROWN} stroke-width="1.5" stroke-dasharray="12 9" stroke-dashoffset={n as f32 * 1.5} opacity={0.28 * (1. - z)} />);
        let mut words = Vec::new();
        for (i, (w, &(x, _))) in LINE1.iter().zip(&lay.line1).enumerate() {
            if i < typed {
                words.push(
                    fframes::svgr!(<g opacity={dim}>{copy(w, X0 + x, Y1, SIZE, INK_TEXT)}</g>),
                );
            }
        }
        for (i, (w, &(x, _))) in LINE2.iter().zip(&lay.line2).enumerate() {
            if i + 6 < typed {
                let focused = i == 0 || (pan > 0. && i >= 3);
                let o = if focused { 1. } else { dim };
                words
                    .push(fframes::svgr!(<g opacity={o}>{copy(w, X0 + x, Y2, SIZE, INK_TEXT)}</g>));
            }
        }
        let caret = if typed < 11 || (n / 6).is_multiple_of(2) {
            let (lx, line_y, ws) = if typed <= 6 {
                (lay.line1[typed - 1], Y1, &LINE1[..])
            } else {
                (lay.line2[typed - 7], Y2, &LINE2[..])
            };
            let _ = ws;
            fframes::svgr!(<rect x={X0 + lx.0 + lx.1 + 8.} y={line_y - 52.} width="5" height="64" fill={ORANGE} />)
        } else {
            Svgr::empty()
        };

        // Random pen around the words while they type.
        let gestures = self
            .gestures
            .iter()
            .filter_map(|(at, hold, stroke)| {
                let age = n as f32 - *at as f32;
                if !(0.0..(*hold as f32 + 6.)).contains(&age) {
                    return None;
                }
                let drawn = (age / 3.).min(1.);
                let gone = ((age - 3. - *hold as f32) / 3.).clamp(0., 1.);
                Some(stroke.draw_window(gone, drawn, e))
            })
            .collect::<Vec<_>>();

        // A selection bracket blinks around the finished question.
        let block_w = (lay.line1[5].0 + lay.line1[5].1).max(lay.line2[4].0 + lay.line2[4].1) + 40.;
        let bracket = if matches!(n, 66..=67 | 69 | 71) {
            let (x, y, w, h) = (X0 - 22., Y1 - 74., block_w, Y2 - Y1 + 104.);
            fframes::svgr!(<g opacity="0.75">
                <path d={format!("M{x:.1} {y:.1} H{:.1} M{x:.1} {:.1} H{:.1}", x + w, y + h, x + w)} stroke={BROWN} stroke-width="2.5" stroke-dasharray="7 7" />
                <path d={format!("M{:.1} {y:.1} h20 M{x:.1} {y:.1} v{h:.1} M{:.1} {:.1} h20 M{:.1} {y:.1} h20 M{:.1} {y:.1} v{h:.1} M{:.1} {:.1} h20", x - 10., x - 10., y + h, x + w - 10., x + w, x + w - 10., y + h)} stroke={INK_TEXT} stroke-width="6" />
            </g>)
        } else {
            Svgr::empty()
        };

        // "stunning": circled, then filled with molten light.
        let circle = Stroke::new(
            &ink::encircle(stun_c[0], stun_c[1], stunning.1 / 2. + 30., 50., 21.),
            7.,
            RED,
            21.,
        )
        .boil(2.)
        .draw(ink::reveal(n as f32, 66., 10.), e);
        let circle2 = Stroke::new(
            &ink::encircle(
                stun_c[0] + 6.,
                stun_c[1] - 3.,
                stunning.1 / 2. + 44.,
                62.,
                29.,
            ),
            3.,
            BROWN,
            29.,
        )
        .boil(2.)
        .draw(ink::reveal(n as f32, 73., 9.), e);
        let fill = ((n as f32 - 82.) / 8.).clamp(0., 1.);
        let molten = if fill > 0. {
            let (wx, wy) = (X0 + stunning.0 - 6., Y2 - 64.);
            let layer = s.molten.draw(
                frame,
                ShaderUniforms::new()
                    .float("uClock", t)
                    .float("uHeat", fill),
            );
            fframes::svgr!(<g opacity={fill}>
                <text x={X0 + stunning.0} y={Y2} font-family="Inter 24pt" font-weight="700" font-size={SIZE} letter-spacing={-SIZE * 0.035}
                    fill={INK_TEXT} stroke={INK_TEXT} stroke-width="5" stroke-linejoin="round">"stunning"</text>
                <defs>
                    <clipPath id="stunning-letters">
                        <text x={X0 + stunning.0} y={Y2} font-family="Inter 24pt" font-weight="700" font-size={SIZE} letter-spacing={-SIZE * 0.035}>"stunning"</text>
                    </clipPath>
                </defs>
                <image href={layer.href()} x={wx} y={wy} width={stunning.1 + 12.} height="84" clip-path="url(#stunning-letters)" />
            </g>)
        } else {
            Svgr::empty()
        };
        let sparkles = (0..11)
            .map(|i| {
                let k = i as f32;
                let at = 86. + k * 2.;
                let age = n as f32 - at;
                if age < 0. {
                    return Svgr::empty();
                }
                let a = k * 2.39 + 0.4;
                let rr = 1. + ink::hash(k) * 0.5;
                let x = stun_c[0] + a.cos() * (stunning.1 / 2. + 34.) * rr;
                let y = stun_c[1] + a.sin() * 62. * rr;
                let pop = (age / 3.).min(1.) * (1. + 0.4 * (-(age - 3.).max(0.) * 0.6).exp());
                let tw = 0.75 + 0.25 * (age * 1.7 + k).sin();
                let size = (6. + ink::hash(k + 4.) * 10.) * pop * tw;
                let color = [ORANGE, RED, BROWN][i % 3];
                ink::sparkle(x, y, size, color)
            })
            .collect::<Vec<_>>();
        let rays = (0..8)
            .map(|i| {
                let k = i as f32;
                let a = -2.6 + k * 0.32 + (ink::hash(k) - 0.5) * 0.15;
                let (r0, r1) = (
                    96. + ink::hash(k + 1.) * 10.,
                    128. + ink::hash(k + 2.) * 30.,
                );
                let (cx, cy) = (stun_c[0], stun_c[1] - 4.);
                let ex = stunning.1 / 2. / 60.;
                let p0 = [cx + a.cos() * r0 * ex, cy + a.sin() * r0 * 0.62];
                let p1 = [cx + a.cos() * r1 * ex, cy + a.sin() * r1 * 0.62];
                Stroke::new(&[p0, p1], 4.5, BROWN, 60. + k)
                    .boil(1.2)
                    .draw(ink::reveal(n as f32, 92. + k * 0.7, 3.), e)
            })
            .collect::<Vec<_>>();

        // "one shot?" gets an orange ring right before the gun.
        let one_ring = Stroke::new(
            &ink::encircle(
                one_c[0],
                one_c[1],
                (shot.0 + shot.1 - one.0) / 2. + 26.,
                48.,
                33.,
            ),
            6.,
            ORANGE,
            33.,
        )
        .boil(2.)
        .draw(ink::reveal(n as f32, 116., 7.), e);

        let streak = if punch > 0. {
            let lines = (0..10)
                .map(|i| {
                    let k = i as f32;
                    let y = 60. + k * 100. + (ink::hash(k + n as f32) - 0.5) * 50.;
                    let len = 200. + punch * 900. * ink::hash(k + 2.);
                    fframes::svgr!(<path d={format!("M{:.1} {y:.1} h{len:.1}", 1440. - len * ink::hash(k + 6.))} stroke={INK_TEXT}
                        stroke-width={2. + 6. * ink::hash(k + 9.)} stroke-linecap="round" opacity={punch * 0.5} />)
                })
                .collect::<Vec<_>>();
            fframes::svgr!(<g>{lines}</g>)
        } else {
            Svgr::empty()
        };

        fframes::svgr!(<g>
            <g transform={camera}>
                {rules}
                {words}
                {caret}
                {bracket}
                {gestures}
                {circle2}
                {circle}
                {molten}
                {rays}
                {sparkles}
                {one_ring}
            </g>
            {streak}
        </g>)
    }
}
