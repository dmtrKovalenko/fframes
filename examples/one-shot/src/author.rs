//! 16.1–19.1 s: a man rises from a chair (stock footage as a yellow heat field):
//! "you are the author."

use std::sync::OnceLock;

use fframes::{FFramesContext, Frame, Svgr};

use crate::ink::{self, CREAM, ORANGE, Stroke};
use crate::{HeatDraw, MAN, Palette, Studio, copy, layout, memo};

const SIZE: f32 = 108.;
const X: f32 = 104.;
const Y1: f32 = 476.;
const Y2: f32 = 600.;
const MAN_X: f32 = 742.;
const GROUND: f32 = 1046.;

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
        let rise = (nf / 55.).min(1.);
        let top = GROUND - MAN.cell[1];
        let man = s.heat(
            &frame,
            ctx,
            MAN,
            HeatDraw {
                index: n.min(MAN.frames - 1),
                palette: Palette::Yellow,
                x: MAN_X,
                y: top,
                scale: 1.,
                gain: 0.92 + 0.12 * rise,
                glow: 0.55 + 0.3 * rise,
                opacity: 1.,
                hot: 0.,
                hot_pos: [0., 0.],
            },
        );
        let body = [MAN_X + 230., top + 380.];

        // Slow light rays behind him and a halo that grows as he stands.
        let rays = (0..14)
            .map(|i| {
                let k = i as f32;
                let a = k / 14. * std::f32::consts::TAU + t * 0.12;
                let spread = 0.07 + ink::hash(k) * 0.05;
                let r = 900.;
                let (cx, cy) = (body[0], body[1] - 160.);
                let d = format!(
                    "M{cx:.1} {cy:.1} L{:.1} {:.1} L{:.1} {:.1}Z",
                    cx + (a - spread).cos() * r,
                    cy + (a - spread).sin() * r,
                    cx + (a + spread).cos() * r,
                    cy + (a + spread).sin() * r
                );
                fframes::svgr!(<path d={d} fill="#f7c51e" opacity={(0.025 + ink::hash(k + 1.) * 0.03) * (0.3 + rise)} />)
            })
            .collect::<Vec<_>>();
        let embers = (0..48)
            .map(|i| {
                let k = i as f32 * 1.9;
                let period = 1.0 + ink::hash(k) * 1.2;
                let a = ((t + ink::hash(k + 1.) * period) % period) / period;
                let x = body[0] - 150. + ink::hash(k + 2.) * 300. + (a * 6. + k).sin() * 20.;
                let y = GROUND - 80. - ink::hash(k + 3.) * 500. - a * 260.;
                let o = (1. - a) * (0.25 + 0.75 * rise) * (0.6 + 0.4 * (t * 20. + k).sin());
                let color = ["#ffe66d", "#ffb703", "#fb6a22"][i % 3];
                fframes::svgr!(<circle cx={x} cy={y} r={1.2 + ink::hash(k + 4.) * 2.6} fill={color} opacity={o} />)
            })
            .collect::<Vec<_>>();
        // The last of the vortex winds around him and fades.
        let orbit = (0..9)
            .map(|i| {
                let k = i as f32;
                let fade = 1. - ink::smoothstep(4., 22., nf);
                if fade <= 0. {
                    return Svgr::empty();
                }
                let r = 140. + k * 12. + nf * 4.;
                let a0 = k * 0.7 + t * (5. - k * 0.3);
                let pts = ink::arc(body[0], body[1] - 80., r * 1.2, r * 0.55, a0, a0 + 0.9, 700. + k);
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
        .draw(ink::reveal(nf, 24., 8.), e);
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
                    <stop offset="0" stop-color="#ffd23f" stop-opacity="0.32" />
                    <stop offset="1" stop-color="#ffd23f" stop-opacity="0" />
                </radialGradient>
            </defs>
            {s.paper(&frame, 1., 0.)}
            {rays}
            <ellipse cx={body[0]} cy={body[1] - 120.} rx={360. + rise * 120.} ry={460. + rise * 120.} fill="url(#author-halo)" opacity={0.3 + 0.7 * rise} />
            <path d={format!("M60 {GROUND:.1} H1380")} stroke={CREAM} stroke-width="1.5" stroke-dasharray="10 12" opacity="0.3" />
            <ellipse cx={body[0]} cy={GROUND + 4.} rx="190" ry="12" fill="#f7c51e" opacity={0.12 + 0.1 * rise} />
            {orbit}
            {man}
            {embers}
            {word("you", X + l1[0].0, Y1, 6., ORANGE)}
            {word("are", X + l1[1].0, Y1, 9., CREAM)}
            {word("the", X + l2[0].0, Y2, 15., CREAM)}
            {word("author.", X + l2[1].0, Y2, 18., CREAM)}
            {ring}
            {sparks}
            {under}
            {under2}
        </g>)
    }
}
