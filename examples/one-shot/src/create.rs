//! 19.1–27 s: "be yourself." / "create." — nineteen real tools (stock photos lifted
//! with Vision) orbit the word, then break into 404 small copies that fly into the
//! fframes wordmark and resolve into the solid logo.

use std::sync::OnceLock;

use fframes::{FFramesContext, Frame, Svgr};

use crate::ink::{self, BROWN, ORANGE, RED, Stroke};
use crate::logo_points::LOGO_POINTS;
use crate::tool_sizes::TOOL_SIZES;
use crate::{INK_TEXT, Studio, copy, measure, memo};

const CREATE: f32 = 24.;
const ASSEMBLE: f32 = 72.;
const RESOLVE: f32 = 92.;
const LOGO_SCALE: f32 = 2.6;
const LOGO_X: f32 = 210.;
const LOGO_Y: f32 = 476.;
const CENTER: [f32; 2] = [720., 560.];
const TILE: f32 = 22.;

#[derive(Debug, Clone, Copy)]
struct Pose {
    x: f32,
    y: f32,
    scale: f32,
    rot: f32,
    depth: f32,
}

#[derive(Debug)]
pub(crate) struct Create {
    widths: OnceLock<(f32, f32, f32)>,
    paths: Vec<&'static str>,
}

fn tool_count() -> usize {
    TOOL_SIZES.len()
}

/// Where tool `i` floats at scene frame `nf` (before assembly).
fn orbit(i: usize, nf: f32) -> Pose {
    let k = i as f32;
    let inner = i < 8;
    let count = if inner { 8. } else { (tool_count() - 8) as f32 };
    let slot = if inner { k } else { k - 8. };
    let since = (nf - CREATE).max(0.) / 24.;
    let speed = if inner { -0.55 } else { 0.42 };
    let a = slot / count * std::f32::consts::TAU + since * speed + if inner { 0.3 } else { 0. };
    let (rx, ry) = if inner {
        (420. + ink::hash(k) * 40., 245.)
    } else {
        (640. + ink::hash(k) * 50., 350.)
    };
    let depth = a.sin();
    let base = if inner { 0.34 } else { 0.42 };
    let x = CENTER[0] + a.cos() * rx;
    let y = CENTER[1] + a.sin() * ry + (nf / 24. * 2.2 + k * 1.7).sin() * 10.;
    let rot = (ink::hash(k + 9.) - 0.5) * 50. + (nf / 24. * 1.3 + k).sin() * 12.;
    Pose {
        x,
        y,
        scale: base + 0.2 * (depth + 1.) / 2.,
        rot,
        depth,
    }
}

/// The orbit with the burst out of the word on "create.".
fn tool_pose(i: usize, nf: f32) -> Pose {
    let p = orbit(i, nf);
    let a = (nf - CREATE - i as f32 * 0.5) / 24.;
    if a <= 0. {
        return Pose { scale: 0., ..p };
    }
    let s = 1. - (-a * 8.).exp() * (a * 15.).cos();
    Pose {
        x: CENTER[0] + (p.x - CENTER[0]) * s,
        y: CENTER[1] + 60. + (p.y - CENTER[1] - 60.) * s,
        scale: p.scale * (a * 10.).min(1.),
        ..p
    }
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    if t < 0.5 {
        4. * t * t * t
    } else {
        1. - (-2. * t + 2.).powi(3) / 2.
    }
}

impl Create {
    pub(crate) fn new() -> Self {
        Self {
            widths: OnceLock::new(),
            paths: include_str!("wordmark.paths").lines().collect(),
        }
    }

    /// Tool `i` centred at (x, y), its longer side `size` px, rotated by `rot` degrees.
    fn sprite(
        &self,
        ctx: &FFramesContext<'_, '_>,
        i: usize,
        [x, y, size, rot]: [f32; 4],
        opacity: f32,
    ) -> Svgr<'static> {
        let name = format!("tool-{i:02}.png");
        let Some(image) = ctx.get_image(&name) else {
            return Svgr::empty();
        };
        let [w, h] = TOOL_SIZES[i];
        let k = size / w.max(h);
        fframes::svgr!(<image href={image.href()} x={-w / 2.} y={-h / 2.} width={w} height={h} opacity={opacity}
            transform={format!("translate({x:.1} {y:.1}) rotate({rot:.1}) scale({k:.4})")} />)
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
        let (be_w, create_w, with_w) = memo(&self.widths, || {
            (
                measure(&mut frame, ctx, "be yourself.", 128.),
                measure(&mut frame, ctx, "create.", 220.),
                measure(&mut frame, ctx, "create with", 62.),
            )
        });

        // ---- words
        let be_a = ((nf + 1.) / 4.).clamp(0., 1.);
        let be_up = ink::smoothstep(CREATE, CREATE + 8., nf);
        let be_out = 1. - ink::smoothstep(CREATE + 14., CREATE + 22., nf);
        let be_scale = 1. - 0.55 * be_up;
        let be_y = 600. - 420. * be_up;
        let be = fframes::svgr!(<g opacity={be_a * be_out}
            transform={format!("translate(720 {be_y:.1}) scale({be_scale:.3}) translate({:.1} 0)", -be_w / 2.)}>
            {copy("be yourself.", 0., 0., 128., INK_TEXT)}
        </g>);
        let be_line = Stroke::new(
            &ink::underline(720. - be_w / 2., 720. + be_w / 2., 640., 801.),
            7.,
            ORANGE,
            801.,
        )
        .draw(ink::reveal(nf, 7., 6.), e);
        let be_line = fframes::svgr!(<g opacity={1. - be_up}>{be_line}</g>);
        // Doodles bloom around "be yourself." and are blown away by "create.".
        let doodles = (0..10)
            .map(|i| {
                let k = i as f32;
                let a = k / 10. * std::f32::consts::TAU + 0.4;
                let (x, y) = (720. + a.cos() * (be_w / 2. + 120.), 560. + a.sin() * 190.);
                let at = 1. + k * 1.6;
                let gone = ink::smoothstep(CREATE - 2., CREATE + 3., nf);
                let push = gone * 260.;
                let stroke = if i % 3 == 0 {
                    return fframes::svgr!(<g opacity={1. - gone}>{ink::sparkle(x + a.cos() * push, y + a.sin() * push, 14. * ((nf - at) / 3.).clamp(0., 1.), [ORANGE, RED, BROWN][i % 3])}</g>);
                } else if i % 3 == 1 {
                    Stroke::new(&ink::arc(x, y, 30., 26., 0., 9., 830. + k), 3.5, BROWN, 830. + k)
                } else {
                    Stroke::new(&ink::gesture(x - 60., y - 40., 120., 80., 840. + k, 5), 3.5, RED, 840. + k)
                };
                fframes::svgr!(<g opacity={1. - gone} transform={format!("translate({:.1} {:.1})", a.cos() * push, a.sin() * push)}>
                    {stroke.draw(ink::reveal(nf, at, 4.), e)}
                </g>)
            })
            .collect::<Vec<_>>();

        let slam = ((nf - CREATE) / 3.).clamp(0., 1.);
        let shrink = ease((nf - ASSEMBLE) / 12.);
        let create_scale = (1. + 0.45 * (1. - slam).powi(2)) * (1. - 0.72 * shrink);
        let create_y = 650. - 240. * shrink;
        let swap = ink::smoothstep(ASSEMBLE + 10., ASSEMBLE + 14., nf);
        let create = if nf >= CREATE {
            fframes::svgr!(<g opacity={slam * (1. - swap)}
                transform={format!("translate(720 {create_y:.1}) scale({create_scale:.3}) translate({:.1} 0)", -create_w / 2.)}>
                {copy("create.", 0., 0., 220., INK_TEXT)}
            </g>)
        } else {
            Svgr::empty()
        };
        let with = fframes::svgr!(<g opacity={swap}>{copy("create with", 720. - with_w / 2., LOGO_Y - 40., 62., INK_TEXT)}</g>);

        // ---- tools in orbit, then the mosaic
        let emit = 1. - ink::smoothstep(ASSEMBLE, ASSEMBLE + 12., nf);
        let mut back = Vec::new();
        let mut front = Vec::new();
        let mut shadows = Vec::new();
        if (CREATE..ASSEMBLE + 12.).contains(&nf) {
            for i in 0..tool_count() {
                let p = tool_pose(i, nf);
                let size = 300. * p.scale * emit;
                if size < 1. {
                    continue;
                }
                let sprite = self.sprite(ctx, i, [p.x, p.y, size, p.rot], 1.);
                shadows.push(fframes::svgr!(<ellipse cx={p.x + 10.} cy={p.y + size * 0.45} rx={size * 0.38} ry={size * 0.07} fill="#2a1d16" opacity="0.16" />));
                if p.depth >= 0. {
                    front.push(sprite);
                } else {
                    back.push(sprite);
                }
            }
        }
        let mosaic = if (ASSEMBLE..RESOLVE + 14.).contains(&nf) {
            LOGO_POINTS
                .iter()
                .enumerate()
                .filter_map(|(j, &[px, py])| {
                    let k = j as f32;
                    let i = j % tool_count();
                    let tx = LOGO_X + px * LOGO_SCALE;
                    let ty = LOGO_Y + py * LOGO_SCALE;
                    let order = px / 392.;
                    let start = ASSEMBLE + order * 8. + ink::hash(k) * 3.;
                    let flight = 9. + ink::hash(k + 1.) * 5.;
                    let p = ((nf - start) / flight).clamp(0., 1.);
                    if nf < start {
                        return None;
                    }
                    let src = tool_pose(i, start);
                    let q = ease(p);
                    let arc = (ink::hash(k + 2.) - 0.5) * 260. * (q * std::f32::consts::PI).sin();
                    let (dx, dy) = (tx - src.x, ty - src.y);
                    let len = (dx * dx + dy * dy).sqrt().max(1.);
                    let x = src.x + dx * q - dy / len * arc;
                    let y = src.y + dy * q + dx / len * arc;
                    let out = ((nf - RESOLVE - order * 6.) / 4.).clamp(0., 1.);
                    let size = (70. + (TILE - 70.) * q) * (1. - out);
                    if size < 1. {
                        return None;
                    }
                    let rot = src.rot + ((ink::hash(k + 3.) - 0.5) * 60. - src.rot) * q;
                    Some(self.sprite(ctx, i, [x, y, size, rot], 1.))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        // ---- the wordmark
        let solid = ink::smoothstep(RESOLVE - 2., RESOLVE + 6., nf);
        let ghost = |idx: usize, at: f32, alpha: f32| {
            let a = (nf - at) / 24.;
            if a <= 0. {
                return Svgr::empty();
            }
            // Once settled, the ghosts flick out and back every second: the logo's own echo.
            let cycle = ((nf - RESOLVE - 30.).max(0.) % 24.) / 6.;
            let flick = if nf > RESOLVE + 30. && cycle < 1. {
                -(cycle * std::f32::consts::PI).sin() * 26. * (idx + 1) as f32
            } else {
                0.
            };
            let slide = -90. * (-a * 9.).exp() * (a * 14.).cos() + flick;
            fframes::svgr!(<path d={self.paths[idx]} fill={ORANGE} fill-opacity={alpha * (a * 8.).min(1.)}
                transform={format!("translate({:.1} {LOGO_Y:.1}) scale({LOGO_SCALE})", LOGO_X + slide)} />)
        };
        let breathe =
            1. + 0.01 * ((t - 4.) * 1.8).sin() * ink::smoothstep(RESOLVE, RESOLVE + 20., nf);
        let logo_c = [LOGO_X + 196. * LOGO_SCALE, LOGO_Y + 50. * LOGO_SCALE];
        let logo = fframes::svgr!(<g transform={format!("translate({:.1} {:.1}) scale({breathe:.4}) translate({:.1} {:.1})", logo_c[0], logo_c[1], -logo_c[0], -logo_c[1])}>
            {ghost(0, RESOLVE + 2., 0.25)}
            {ghost(1, RESOLVE + 5., 0.55)}
            <path d={self.paths[2]} fill={INK_TEXT} opacity={solid}
                transform={format!("translate({LOGO_X:.1} {LOGO_Y:.1}) scale({LOGO_SCALE})")} />
        </g>);
        let swoosh = Stroke::new(
            &[
                [LOGO_X + 40., LOGO_Y + 282.],
                [LOGO_X + 400., LOGO_Y + 270.],
                [LOGO_X + 800., LOGO_Y + 266.],
                [LOGO_X + 1010., LOGO_Y + 258.],
                [LOGO_X + 1050., LOGO_Y + 236.],
            ],
            9.,
            ORANGE,
            811.,
        )
        .draw(ink::reveal(nf, RESOLVE + 8., 8.), e);
        let sparkles = (0..12)
            .map(|i| {
                let k = i as f32;
                let at = if nf < ASSEMBLE {
                    CREATE + 4. + k * 3.5
                } else {
                    RESOLVE + 10. + k * 4.
                };
                let a = (nf - at) / 24.;
                if a <= 0. {
                    return Svgr::empty();
                }
                let (x, y) = if nf < ASSEMBLE {
                    let p = tool_pose((i * 5) % tool_count(), nf);
                    (p.x + 70., p.y - 60.)
                } else {
                    (
                        LOGO_X + ink::hash(k) * 1020.,
                        LOGO_Y - 30. + ink::hash(k + 1.) * 330.,
                    )
                };
                let life = (a * 6.).min(1.) * (1. - ((a - 0.8) * 3.).clamp(0., 1.));
                let tw = 0.75 + 0.25 * (t * 9. + k).sin();
                ink::sparkle(
                    x,
                    y,
                    (8. + ink::hash(k + 2.) * 12.) * life * tw,
                    [ORANGE, RED, BROWN][i % 3],
                )
            })
            .collect::<Vec<_>>();
        // A slow outer ring of small tools keeps the end card alive.
        let halo = if nf > RESOLVE {
            let a = ink::smoothstep(RESOLVE, RESOLVE + 20., nf);
            (0..tool_count())
                .map(|i| {
                    let k = i as f32;
                    let ang = k / tool_count() as f32 * std::f32::consts::TAU + t * 0.18;
                    let x = 720. + ang.cos() * 690.;
                    let y = 600. + ang.sin() * 420.;
                    let spin = (t * 20. + k * 40.) % 360.;
                    self.sprite(ctx, i, [x, y, 64. + ink::hash(k) * 24., spin], 0.4 * a)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let guide_on = ink::smoothstep(CREATE + 2., CREATE + 12., nf)
            * (1. - ink::smoothstep(ASSEMBLE, ASSEMBLE + 6., nf));
        let guides = if guide_on > 0. {
            let dash = nf * 2.;
            fframes::svgr!(<g fill="none" stroke={BROWN} stroke-width="1.5" stroke-dasharray="10 12" opacity={0.3 * guide_on}>
                <ellipse cx={CENTER[0]} cy={CENTER[1]} rx="440" ry="245" stroke-dashoffset={-dash} />
                <ellipse cx={CENTER[0]} cy={CENTER[1]} rx="665" ry="350" stroke-dashoffset={dash} />
            </g>)
        } else {
            Svgr::empty()
        };
        let suck = (nf - ASSEMBLE) / 14.;
        let suction = if (0. ..1.).contains(&suck) {
            let r = 760. * (1. - ease(suck));
            fframes::svgr!(<ellipse cx={CENTER[0]} cy={CENTER[1]} rx={r.max(1.)} ry={(r * 0.55).max(1.)} fill="none" stroke={ORANGE}
                stroke-width={2. + 6. * suck} opacity={0.6 * (1. - suck)} />)
        } else {
            Svgr::empty()
        };
        let push = 1.
            + 0.04
                * ink::smoothstep(CREATE, ASSEMBLE, nf)
                * (1. - ink::smoothstep(ASSEMBLE, ASSEMBLE + 10., nf))
            + 0.03 * ink::smoothstep(RESOLVE, 189., nf);
        fframes::svgr!(<g>
            <defs>
                <filter id="create-soft" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="6" /></filter>
            </defs>
            {s.paper(&frame, 0., 0.12)}
            <g transform={format!("translate(720 540) scale({push:.4}) translate(-720 -540)")}>
                {halo}
                {guides}
                {suction}
                {if shadows.is_empty() { Svgr::empty() } else { fframes::svgr!(<g filter="url(#create-soft)">{shadows}</g>) }}
                {back}
                {doodles}
                {be}
                {be_line}
                {create}
                {front}
                {with}
                {logo}
                {mosaic}
                {swoosh}
                {sparkles}
            </g>
        </g>)
    }
}
