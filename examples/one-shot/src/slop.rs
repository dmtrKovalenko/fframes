//! 11.1–14.1 s: a pot of porridge (stock footage) stirred at 52 px, buzzing pixel
//! flies, and "slop." drowning in ink. The soundtrack dips at 13 s; the drips
//! speed up there.

use std::fmt::Write;
use std::sync::OnceLock;

use fframes::{FFramesContext, Frame, ShaderUniforms, Svgr};

use crate::ink::{self, BLACK, BROWN, RED, Stroke};
use crate::{INK_TEXT, Studio, copy, layout, measure, memo};

const POT: [f32; 2] = [1066., 486.];
const PX: f32 = 8.;
const N: f32 = 52.;
const WORD: (f32, f32) = (96., 760.);
const WORD_SIZE: f32 = 270.;
const LEAD: (f32, f32) = (104., 470.);
const LEAD_SIZE: f32 = 84.;
const SLAM: f32 = 12.;

// Palette of the pixel pot (from tools/prep.py's quantised frames).
const SLOP_COLORS: [&str; 5] = ["#cdbba1", "#b48f6c", "#a2683f", "#8a7b6b", "#e2d6c2"];

#[derive(Debug)]
pub(crate) struct Slop {
    widths: OnceLock<(f32, Vec<(f32, f32)>)>,
}

fn pulse(n: f32, at: f32, len: f32) -> f32 {
    let a = (n - at) / len;
    if a < 0. {
        0.
    } else {
        (a.min(1.) * 1.15).min(1.)
    }
}

impl Slop {
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
        let t = frame.seconds();
        let e = ink::exposure(frame.global_index);
        let (slop_w, lead_words) = memo(&self.widths, || {
            (
                measure(&mut frame, ctx, "slop.", WORD_SIZE),
                layout(&mut frame, ctx, &["we", "call", "it"], LEAD_SIZE),
            )
        });
        let lead_w = lead_words[2].0 + lead_words[2].1;
        // The dip in the music: everything sags.
        let sag = ink::smoothstep(45., 69., nf);

        // ---- the pot
        let pot = ctx.get_image("slop.png").map_or_else(Svgr::empty, |atlas| {
            let i = (n % 60) as f32;
            let cell = [(i % 10.) * N, (i / 10.).floor() * N];
            let layer = s.pixel.draw(
                &frame,
                ShaderUniforms::new()
                    .image("uAtlas", atlas)
                    .float2("uCell", cell[0], cell[1])
                    .float("uN", N)
                    .float("uSick", 0.35 + 0.25 * sag)
                    .float("uWobble", 0.25 + 0.4 * sag)
                    .float("uClock", t),
            );
            let size = N * PX;
            let tilt = (t * 2.3).sin() * 2.5;
            let bob = (t * 3.1).sin() * 6. + sag * 34.;
            fframes::svgr!(<g transform={format!("translate({:.1} {:.1}) rotate({tilt:.2})", POT[0], POT[1] + bob)}>
                <ellipse cx="0" cy={size / 2. + 14.} rx={size * 0.42} ry="16" fill={BLACK} opacity="0.12" />
                <image href={layer.href()} x={-size / 2.} y={-size / 2.} width={size} height={size} />
            </g>)
        });
        let pixel_drips = (0..14)
            .map(|i| {
                let k = i as f32 * 1.9;
                let period = 18. + ink::hash(k) * 16.;
                let phase = (nf + ink::hash(k + 1.) * period) % period;
                let a = phase / 24.;
                let col = ((ink::hash(k + 2.) - 0.5) * 34.).round();
                let x = POT[0] + col * PX;
                let y0 = POT[1] + (N / 2. - 3. + (col.abs() / 26.).powi(2) * -10.) * PX + sag * 34.;
                let y = y0 + 0.5 * 2200. * a * a;
                let o = 1. - phase / period;
                fframes::svgr!(<rect x={x} y={(y / PX).round() * PX} width={PX} height={PX * (1. + (i % 2) as f32)}
                    fill={SLOP_COLORS[i % 5]} opacity={o} />)
            })
            .collect::<Vec<_>>();
        let flies = (0..4)
            .map(|i| {
                let k = i as f32;
                let speed = 2.6 + k * 0.7 - sag * 1.2;
                let a = t * speed + k * 1.7;
                let r = 250. + k * 22.;
                let x = POT[0] + a.cos() * r + (t * 17. + k).sin() * 16.;
                let y = POT[1] - 40. + (a * 1.3).sin() * r * 0.55 + (t * 23. + k * 3.).cos() * 12.;
                let x = (x / 4.).round() * 4.;
                let y = (y / 4.).round() * 4.;
                let wing = if (n + i).is_multiple_of(2) { 0. } else { 6. };
                fframes::svgr!(<g>
                    <rect x={x} y={y} width="15" height="9" fill={BLACK} />
                    <rect x={x + 12.} y={y + 3.} width="5" height="5" fill={BLACK} />
                    <rect x={x - 4. + wing * 0.6} y={y - 9.} width="9" height="9" fill="#7d8389" opacity="0.65" />
                    <rect x={x + 8. - wing * 0.6} y={y - 9.} width="9" height="9" fill="#7d8389" opacity="0.65" />
                </g>)
            })
            .collect::<Vec<_>>();
        let steam = (0..3)
            .map(|i| {
                let k = i as f32;
                let x0 = POT[0] - 90. + k * 90.;
                let mut d = String::new();
                for j in 0..14 {
                    let y = POT[1] - N * PX / 2. - 20. - j as f32 * 16.;
                    let x = x0 + ((j as f32 * 0.7 - t * 4. + k * 2.).sin() * 18. / PX).round() * PX;
                    let _ = write!(d, "{}{x:.0} {y:.0}h{PX}", if j == 0 { "M" } else { "L" });
                }
                fframes::svgr!(<path d={d} fill="none" stroke="#9c9182" stroke-width={PX * 0.75} opacity={0.35 - k * 0.07} />)
            })
            .collect::<Vec<_>>();

        // ---- the words and their ink
        let slam = pulse(nf, SLAM, 3.);
        let scale = if slam <= 0. {
            1.6
        } else {
            1. + 0.6 * (1. - slam).powi(2)
                - 0.06
                    * ((nf - SLAM - 3.).max(0.) * 0.5).sin()
                    * (-(nf - SLAM - 3.).max(0.) * 0.25).exp()
        };
        let word_c = [WORD.0 + slop_w / 2., WORD.1 - WORD_SIZE * 0.3];
        let splats = [
            (SLAM, word_c[0] - 210., word_c[1] + 10., 170., BLACK, 3.1),
            (SLAM, word_c[0] + 20., word_c[1] - 10., 190., BLACK, 5.3),
            (
                SLAM + 0.5,
                word_c[0] + 250.,
                word_c[1] + 14.,
                160.,
                BLACK,
                7.9,
            ),
            (SLAM + 1., word_c[0] + 300., word_c[1] - 150., 96., RED, 4.7),
            (24., 210., 900., 120., BROWN, 6.2),
            (36., 640., 320., 70., RED, 8.8),
            (36., 860., 930., 54., RED, 9.9),
            (41., 360., 980., 40., BLACK, 11.3),
            (48., 880., 610., 64., BLACK, 12.7),
            (60., 120., 600., 50., RED, 14.1),
        ]
        .iter()
        .map(|&(at, x, y, r, color, seed)| {
            let g = pulse(nf, at, 2.5) * (1. + 0.12 * (-(nf - at - 2.5).max(0.) * 0.4).exp());
            ink::splat(x, y, r, seed, g, color)
        })
        .collect::<Vec<_>>();
        let drips = (0..9)
            .map(|i| {
                let k = i as f32;
                let x = WORD.0 + 30. + k * (slop_w - 60.) / 8. + (ink::hash(k) - 0.5) * 24.;
                let y = WORD.1 - 6. + ink::hash(k + 1.) * 10.;
                let start = SLAM + 2. + ink::hash(k + 2.) * 10.;
                let a = ((nf - start) / 24.).max(0.);
                let speed = 50. + ink::hash(k + 3.) * 140.;
                let len = speed * a * (1. + 2.2 * sag) + if a > 0. { 6. } else { 0. };
                ink::drip(x, y, len.min(300.), 4. + ink::hash(k + 4.) * 6., BLACK)
            })
            .collect::<Vec<_>>();
        let dots = (0..46)
            .filter_map(|i| {
                let k = i as f32 * 2.9;
                let at = SLAM + ink::hash(k) * 50.;
                if nf < at {
                    return None;
                }
                let x = 40. + ink::hash(k + 1.) * 1360.;
                let y = 60. + ink::hash(k + 2.) * 960.;
                let r = 1.5 + ink::hash(k + 3.).powi(3) * 9.;
                let color = [BLACK, RED, BROWN][i % 3];
                Some(fframes::svgr!(<circle cx={x} cy={y} r={r} fill={color} />))
            })
            .collect::<Vec<_>>();
        let ring = Stroke::new(
            &ink::encircle(word_c[0], word_c[1], slop_w / 2. + 60., 150., 61.),
            10.,
            RED,
            61.,
        )
        .boil(3.)
        .draw(ink::reveal(nf, 28., 9.), e);
        let lead_line = Stroke::new(
            &ink::underline(LEAD.0, LEAD.0 + lead_w, LEAD.1 + 24., 63.),
            5.,
            BROWN,
            63.,
        )
        .draw(ink::reveal(nf, 6., 5.), e);
        let scrawl = Stroke::new(
            &ink::gesture(860., 760., 180., 120., 71., 7),
            5.,
            BLACK,
            71.,
        )
        .boil(2.)
        .draw(ink::reveal(nf, 40., 6.), e);

        let lead = ["we", "call", "it"]
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let on = nf >= i as f32 * 2.;
                let x = LEAD.0 + lead_words[i].0;
                if on {
                    copy(w, x, LEAD.1, LEAD_SIZE, INK_TEXT)
                } else {
                    Svgr::empty()
                }
            })
            .collect::<Vec<_>>();
        let word = if nf >= SLAM {
            fframes::svgr!(<g transform={format!("translate({:.1} {:.1}) scale({scale:.4}) translate({:.1} {:.1})", word_c[0], word_c[1], -word_c[0], -word_c[1])}>
                <g filter="url(#slop-bleed)">{copy("slop.", WORD.0, WORD.1, WORD_SIZE, "#ece5d6")}</g>
            </g>)
        } else {
            Svgr::empty()
        };
        let seed = (e % 7) as f32 + 1.;
        fframes::svgr!(<g>
            <defs>
                <filter id="slop-bleed" x="-5%" y="-10%" width="110%" height="130%">
                    <feTurbulence type="fractalNoise" baseFrequency="0.035" numOctaves="2" seed={seed} result="noise" />
                    <feDisplacementMap in="SourceGraphic" in2="noise" scale="9" xChannelSelector="R" yChannelSelector="G" />
                </filter>
            </defs>
            {s.paper(&frame, 0., 0.2 + 0.4 * sag)}
            {steam}
            {pot}
            {pixel_drips}
            {splats}
            {dots}
            {lead}
            {lead_line}
            {drips}
            {word}
            {ring}
            {scrawl}
            {flies}
        </g>)
    }
}
