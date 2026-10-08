//! 5.6–8.1 s: an orange toy pistol (stock footage, rendered as a flat heat field)
//! fires on the drop. The bullet burns "you can't." into the dark.

use std::sync::OnceLock;

use fframes::{Color, FFramesContext, Frame, ShaderUniforms, Svgr, Transform, animation::Easing};

use crate::ink::{self, CREAM, ORANGE, RED, Stroke};
use crate::{GUN, HeatDraw, Palette, Studio, copy, measure, memo, shake};

const GS: f32 = 0.95;
const MUZZLE: [f32; 2] = [690., 500.];
const MUZZLE_CELL: [f32; 2] = [685., 112.];
const GRIP_CELL: [f32; 2] = [200., 380.];
const SHOT: f32 = 0.5;
const TEXT_X: f32 = 742.;
const TEXT_Y: f32 = 528.;
const TEXT_SIZE: f32 = 98.;
const TARGET: [f32; 2] = [1230., 500.];

#[derive(Debug)]
pub(crate) struct Gun {
    widths: OnceLock<(f32, f32)>,
    bursts: Vec<Stroke>,
    target: Vec<Stroke>,
    curls: Vec<Stroke>,
}

/// Recoil angle and kick-back at `age` seconds after the shot.
fn recoil(age: f32) -> (f32, f32) {
    if age < 0. {
        return (0., 0.);
    }
    let rise = ink::smoothstep(0., 0.07, age);
    let a = (age - 0.07).max(0.);
    let settle = (-a * 6.).exp() * (a * 14.).cos();
    (-18. * rise * settle, -48. * rise * (-a * 8.).exp())
}

impl Gun {
    pub(crate) fn new() -> Self {
        let bursts = (0..8)
            .map(|i| {
                let k = i as f32;
                let a = -1.25 + k * 0.36 + (ink::hash(k) - 0.5) * 0.2;
                let r0 = 70. + ink::hash(k + 1.) * 30.;
                let r1 = r0 + 60. + ink::hash(k + 2.) * 70.;
                let (cx, cy) = (MUZZLE[0] + 40., MUZZLE[1]);
                Stroke::new(
                    &[
                        [cx + a.cos() * r0, cy + a.sin() * r0],
                        [cx + a.cos() * r1, cy + a.sin() * r1],
                    ],
                    6.,
                    ORANGE,
                    300. + k,
                )
                .taper(0.3, 0.5)
            })
            .collect();
        let [tx, ty] = TARGET;
        let target = vec![
            Stroke::new(&ink::encircle(tx, ty, 120., 120., 71.), 4., RED, 71.),
            Stroke::new(
                &ink::encircle(tx + 3., ty - 2., 62., 60., 77.),
                3.,
                RED,
                77.,
            ),
            Stroke::new(&[[tx - 175., ty + 6.], [tx - 40., ty + 2.]], 3.5, RED, 81.),
            Stroke::new(&[[tx + 40., ty - 2.], [tx + 170., ty + 4.]], 3.5, RED, 82.),
            Stroke::new(&[[tx + 4., ty - 170.], [tx, ty - 40.]], 3.5, RED, 83.),
            Stroke::new(&[[tx - 2., ty + 40.], [tx + 5., ty + 168.]], 3.5, RED, 84.),
        ];
        // Smoke drawn as pen curls that rise off the barrel.
        let curls = (0..5)
            .map(|i| {
                let k = i as f32;
                let x = MUZZLE[0] - 10. + k * 26.;
                let ctrl: Vec<[f32; 2]> = (0..7)
                    .map(|j| {
                        let jj = j as f32;
                        [
                            x + (jj * 1.9 + k).sin() * (14. + jj * 5.) + jj * 6.,
                            MUZZLE[1] - 18. - jj * 38.,
                        ]
                    })
                    .collect();
                Stroke::new(&ctrl, 3. + (i % 2) as f32 * 2., CREAM, 90. + k)
                    .boil(2.2)
                    .taper(0.2, 0.6)
            })
            .collect();
        Self {
            widths: OnceLock::new(),
            bursts,
            target,
            curls,
        }
    }

    pub(crate) fn render<'a>(
        &self,
        mut frame: Frame,
        ctx: &FFramesContext<'a, '_>,
        s: &Studio,
    ) -> Svgr<'a> {
        let n = frame.index;
        let t = frame.seconds();
        let age = t - SHOT;
        let e = ink::exposure(frame.global_index);
        let (you, cant) = memo(&self.widths, || {
            (
                measure(&mut frame, ctx, "you ", TEXT_SIZE),
                measure(&mut frame, ctx, "can't", TEXT_SIZE),
            )
        });

        let lower = ink::smoothstep(1.2, 2.5, t) * 7.;
        let x0 = MUZZLE[0] - MUZZLE_CELL[0] * GS;
        let y0 = MUZZLE[1] - MUZZLE_CELL[1] * GS;
        let pivot = [x0 + GRIP_CELL[0] * GS, y0 + GRIP_CELL[1] * GS];
        let sway = (t * 2.1).sin() * 1.2;
        let pose = |a: f32| {
            let (kick, back) = recoil(a);
            format!(
                "translate({back:.2} {:.2}) rotate({:.2} {:.1} {:.1})",
                lower * 3.,
                kick + lower + sway,
                pivot[0],
                pivot[1]
            )
        };
        let hot = if age >= 0. { (-age * 1.6).exp() } else { 0. };
        let draw = |opacity: f32, palette: Palette| {
            s.heat(
                &frame,
                ctx,
                GUN,
                HeatDraw {
                    index: n.min(GUN.frames - 1),
                    palette,
                    x: x0,
                    y: y0,
                    scale: GS,
                    gain: 1.,
                    glow: 0.35,
                    opacity,
                    hot,
                    hot_pos: [MUZZLE_CELL[0] - 50., MUZZLE_CELL[1]],
                },
            )
        };
        // Two ghosts trail the recoil, the same echo the wordmark uses.
        let echoes = if (0.02..0.3).contains(&age) {
            let fade = 1. - age / 0.3;
            fframes::svgr!(<g>
                <g transform={pose(age - 0.05)}>{draw(0.35 * fade, Palette::Echo)}</g>
                <g transform={pose(age - 0.1)}>{draw(0.18 * fade, Palette::Echo)}</g>
            </g>)
        } else {
            Svgr::empty()
        };
        let flash = if (0. ..0.21).contains(&age) {
            let layer = s.muzzle.draw(
                &frame,
                ShaderUniforms::new()
                    .float("uAge", age * 24.)
                    .float("uSeed", 1.7),
            );
            fframes::svgr!(<image href={layer.href()} x={MUZZLE[0] - 330.} y={MUZZLE[1] - 220.} width="720" height="440" />)
        } else {
            Svgr::empty()
        };

        // The bullet and its burning trail.
        let bx = if age < 0. {
            MUZZLE[0]
        } else {
            MUZZLE[0] + 60. + age * 24. * 300.
        };
        let bullet = if (0. ..0.15).contains(&age) {
            let tail = (bx - 760.).max(MUZZLE[0]);
            fframes::svgr!(<g>
                <rect x={tail} y={MUZZLE[1] - 4.} width={(bx - tail).max(1.)} height="8" fill="url(#gun-trail)" />
                <rect x={bx - 44.} y={MUZZLE[1] - 6.} width="44" height="12" rx="6" fill="#fff6dd" />
                <rect x={bx - 44.} y={MUZZLE[1] - 6.} width="44" height="12" rx="6" fill="none" stroke={ORANGE} stroke-width="3" />
            </g>)
        } else if (0.15..0.4).contains(&age) {
            let fade = 1. - (age - 0.15) / 0.25;
            fframes::svgr!(<rect x={MUZZLE[0]} y={MUZZLE[1] - 2.} width={1440. - MUZZLE[0]} height="4" fill="url(#gun-trail-rest)" opacity={fade} />)
        } else {
            Svgr::empty()
        };
        let reveal_to = if age < 0. { TEXT_X - 10. } else { bx };

        let sparks = if (0. ..0.6).contains(&age) {
            (0..30)
                .filter_map(|i| {
                    let k = i as f32 * 1.31;
                    let life = 0.18 + ink::hash(k) * 0.35;
                    if age > life {
                        return None;
                    }
                    let a = (ink::hash(k + 1.) - 0.5) * 1.6 + if i % 5 == 0 { 3.1 } else { 0. };
                    let v = 500. + ink::hash(k + 2.) * 1300.;
                    let (vx, vy) = (a.cos() * v, a.sin() * v);
                    let x = MUZZLE[0] + 50. + vx * age;
                    let y = MUZZLE[1] + vy * age + 900. * age * age;
                    let (tx, ty) = (vx * 0.018, (vy + 1800. * age) * 0.018);
                    let color = ["#fff3c4", "#ffc23a", "#fb6a22"][i % 3];
                    Some(fframes::svgr!(<path d={format!("M{x:.1} {y:.1} l{:.1} {:.1}", -tx, -ty)} stroke={color}
                        stroke-width={1.5 + ink::hash(k + 3.) * 2.5} stroke-linecap="round" opacity={1. - age / life} />))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let curls = self
            .curls
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let born = SHOT + 0.12 + i as f32 * 0.09;
                let a = t - born;
                if a < 0. {
                    return Svgr::empty();
                }
                let drawn = (a / 0.45).min(1.);
                let gone = ((a - 0.5) / 0.7).clamp(0., 1.);
                let lift = -a * 50.;
                fframes::svgr!(<g transform={format!("translate({:.1} {lift:.1})", a * 30.)} opacity="0.55">{c.draw_window(gone, drawn, e)}</g>)
            })
            .collect::<Vec<_>>();
        let casing = if (0. ..0.9).contains(&age) {
            let (sx, sy) = (x0 + 450. * GS, y0 + 60. * GS);
            let floor = 930.;
            let (x, y, r) = {
                let x = sx - 150. * age;
                let y = sy - 640. * age + 1900. * age * age;
                if y < floor {
                    (x, y, age * 1500.)
                } else {
                    // First bounce: about 0.47 s in, a third of the speed.
                    let tb = 0.47;
                    let a2 = age - tb;
                    (
                        sx - 150. * tb - 60. * a2,
                        floor - 210. * a2 + 1900. * a2 * a2,
                        tb * 1500. + a2 * 500.,
                    )
                }
            };
            fframes::svgr!(<g transform={format!("translate({x:.1} {:.1}) rotate({r:.1})", y.min(floor + 2.))}>
                <rect x="-9" y="-4" width="18" height="8" rx="2" fill="#e9a743" stroke="#7a3b0e" stroke-width="1.5" />
                <rect x="5" y="-4" width="4" height="8" fill="#b9741f" />
            </g>)
        } else {
            Svgr::empty()
        };
        let embers = (0..22)
            .filter_map(|i| {
                let k = i as f32 * 3.7;
                let born = SHOT + ink::hash(k) * 1.6;
                let a = t - born;
                if !(0. ..1.2).contains(&a) {
                    return None;
                }
                let x = MUZZLE[0] + 20. + ink::hash(k + 1.) * 160. + (a * 4. + k).sin() * 10.;
                let y = MUZZLE[1] - 10. - a * (60. + ink::hash(k + 2.) * 120.);
                let flicker = 0.5 + 0.5 * (a * 30. + k).sin();
                Some(fframes::svgr!(<circle cx={x} cy={y} r={1.2 + ink::hash(k + 3.) * 2.} fill="#ffb347" opacity={(1. - a / 1.2) * flicker} />))
            })
            .collect::<Vec<_>>();

        // Pen: a target drawn while it aims, action lines on the shot, a splat
        // where the bullet leaves the frame and "can't" underlined twice.
        let target = self
            .target
            .iter()
            .enumerate()
            .map(|(i, st)| {
                let on = ink::reveal(n as f32, 2. + i as f32 * 1.3, 4.);
                let off = ((n as f32 - 14. - i as f32 * 0.5) / 4.).clamp(0., 1.);
                st.draw_window(off, on, e)
            })
            .collect::<Vec<_>>();
        let shock = if (0. ..0.3).contains(&age) {
            let r = 30. + age * 900.;
            let ring = Stroke::new(
                &ink::arc(MUZZLE[0] + 40., MUZZLE[1], r, r * 0.8, -1.4, 1.4, 340.),
                4. * (1. - age / 0.3) + 1.,
                CREAM,
                340.,
            )
            .taper(0.3, 0.3)
            .draw(1., e);
            fframes::svgr!(<g opacity={1. - age / 0.3}>{ring}</g>)
        } else {
            Svgr::empty()
        };
        let bursts = self
            .bursts
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let on = ink::reveal(n as f32, 12. + (i % 3) as f32 * 0.5, 2.);
                let off = ((n as f32 - 18.) / 3.).clamp(0., 1.);
                b.draw_window(off, on, e)
            })
            .collect::<Vec<_>>();
        let exit_splat = ink::splat(
            1452.,
            MUZZLE[1] + 4.,
            64.,
            7.7,
            ((age - 0.06) * 14.).clamp(0., 1.) * (1. + 0.15 * (-(age - 0.13).max(0.) * 8.).exp()),
            RED,
        );
        let target_splat = ink::splat(
            TARGET[0],
            TARGET[1],
            30.,
            3.3,
            ((age - 0.05) * 18.).clamp(0., 1.),
            RED,
        );
        let cant_x = TEXT_X + you;
        let under1 = Stroke::new(
            &ink::underline(cant_x, cant_x + cant, TEXT_Y + 22., 3.),
            7.,
            RED,
            3.,
        )
        .draw(ink::reveal(n as f32, 21., 5.), e);
        let under2 = Stroke::new(
            &ink::underline(cant_x + 10., cant_x + cant - 6., TEXT_Y + 42., 9.),
            5.,
            RED,
            9.,
        )
        .draw(ink::reveal(n as f32, 26., 5.), e);
        let aim = if (6..12).contains(&n) {
            let dots = (0..22)
                .map(|i| {
                    let x = MUZZLE[0] + 80. + i as f32 * 32. + (n % 2) as f32 * 8.;
                    fframes::svgr!(<circle cx={x} cy={MUZZLE[1]} r="2.2" fill={CREAM} opacity="0.45" />)
                })
                .collect::<Vec<_>>();
            fframes::svgr!(<g>{dots}</g>)
        } else {
            Svgr::empty()
        };

        let (dx, dy) = shake(t, SHOT, 0.32, 18., 5.);
        let push = 1. + 0.05 * ink::smoothstep(0.7, 2.5, t);
        let tint = match n {
            12 => 0.34,
            13 => 0.12,
            _ => 0.,
        };
        fframes::svgr!(<g>
            <defs>
                <linearGradient id="gun-trail" x1="0" x2="1" y1="0" y2="0">
                    <stop offset="0" stop-color="#fb6a22" stop-opacity="0" />
                    <stop offset="0.7" stop-color="#ffb347" stop-opacity="0.8" />
                    <stop offset="1" stop-color="#fff6dd" stop-opacity="1" />
                </linearGradient>
                <linearGradient id="gun-trail-rest" x1="0" x2="1" y1="0" y2="0">
                    <stop offset="0" stop-color="#fb6a22" stop-opacity="0.7" />
                    <stop offset="1" stop-color="#fb6a22" stop-opacity="0" />
                </linearGradient>
                <clipPath id="gun-reveal"><rect x="0" y="0" width={reveal_to.max(1.)} height="1080" /></clipPath>
                <filter id="gun-glow" x="-20%" y="-50%" width="140%" height="200%"><feGaussianBlur stdDeviation="14" /></filter>
            </defs>
            {s.paper(&frame, 1., 0.)}
            <g transform={format!("translate({:.1} {:.1}) scale({push:.4}) translate(-720 -540)", 720. + dx, 540. + dy)}>
                {aim}
                {target}
                {target_splat}
                {curls}
                <g clip-path="url(#gun-reveal)">
                    <g filter="url(#gun-glow)" opacity={(1. - age * 1.4).clamp(0., 0.9)}>
                        {copy("you can't.", TEXT_X, TEXT_Y, TEXT_SIZE, ORANGE)}
                    </g>
                    <g fill={frame.animate(fframes::timeline!(
                        at 0.54 => 1.0, animate Color::hex("#ffd04a") => Color::hex("#f3eee1"), Easing::EaseOut,
                    ))}>
                        <text x={TEXT_X} y={TEXT_Y} font-family="Inter 24pt" font-weight="700" font-size={TEXT_SIZE}
                            letter-spacing={-TEXT_SIZE * 0.035}>"you can't."</text>
                    </g>
                </g>
                {under1}
                {under2}
                <g transform={frame.animate(fframes::timeline!(
                    at 0.0, animate Transform::translate(-430, 0) => Transform::translate(0, 0),
                        Easing::Spring { mass: 1.0, stiffness: 380.0, damping: 26.0 },
                ))}>
                    {echoes}
                    <g transform={pose(age)}>
                        {draw(1., Palette::Orange)}
                        {flash}
                    </g>
                </g>
                {casing}
                {bullet}
                {sparks}
                {embers}
                {shock}
                {bursts}
                {exit_splat}
            </g>
            <rect width="1440" height="1080" fill="#ff9a4d" opacity={tint} />
        </g>)
    }
}
