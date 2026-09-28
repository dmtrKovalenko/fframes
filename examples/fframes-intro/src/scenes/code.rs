//! The code of a video is typed out while its output renders next to it.
//! The lift ends in a bar of near silence reading "rendered on the".

use fframes::{Duration, FFramesContext, Frame, Scene, Svgr};

use crate::beat::*;
use crate::ui::*;

beat_scene!(CodeScene, Some(32.0), Some(48.0));

const KW: &str = ORANGE;
const TY: &str = "#d9c7a8";
const PL: &str = BONE;
const PU: &str = "#8f8a82";
const ST: &str = "#f3a676";

/// The code, as colored tokens per line.
const CODE: &[&[(&str, &str)]] = &[
    &[("impl ", KW), ("Video ", TY), ("for ", KW), ("Intro ", TY), ("{", PU)],
    &[("    const ", KW), ("FPS", PL), (": ", PU), ("usize ", TY), ("= ", PU), ("60", ST), (";", PU)],
    &[],
    &[("    fn ", KW), ("render_frame", PL), ("(", PU), ("&self", KW), (", ", PU), ("frame", PL), (": ", PU), ("Frame", TY), (") -> ", PU), ("Svgr ", TY), ("{", PU)],
    &[("        let ", KW), ("y ", PL), ("= ", PU), ("spring", PL), ("(frame.", PU), ("seconds", PL), ("());", PU)],
    &[("        svgr!", KW), ("(<", PU), ("text ", PL), ("y", TY), ("={y}>", PU), ("\"EVERY FRAME\"", ST), ("</", PU), ("text", PL), (">)", PU)],
    &[("    }", PU)],
    &[("}", PU)],
];

const CODE_X: f32 = 150.0;
const CODE_Y: f32 = 300.0;
const CODE_SIZE: f32 = 34.0;
const LINE_H: f32 = 56.0;
const ADV: f32 = CODE_SIZE * 0.6;

impl Scene for CodeScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        if lb >= 46.0 - 32.0 {
            return silence(lb - 14.0);
        }
        let total: usize = CODE.iter().map(|l| l.iter().map(|(t, _)| t.chars().count()).sum::<usize>()).sum();
        let typed = (expo_out(prog(lb, 0.0, 7.0)).powf(0.7) * total as f32) as usize;
        let mut left = typed;
        let mut rows = Vec::new();
        let mut caret = (CODE_X, CODE_Y);
        for (li, line) in CODE.iter().enumerate() {
            let y = CODE_Y + li as f32 * LINE_H;
            let mut col = 0usize;
            let mut spans = Vec::new();
            for (text, color) in line.iter() {
                let n = text.chars().count();
                let take = n.min(left);
                if take > 0 {
                    let part: String = text.chars().take(take).collect();
                    let lead = part.chars().take_while(|c| *c == ' ').count();
                    let part = part.trim_start().to_owned();
                    let x = CODE_X + (col + lead) as f32 * ADV;
                    if !part.is_empty() {
                        spans.push(fframes::svgr!(
                            <text x={x} y={y} font-family={MONO} font-weight="500" font-size={CODE_SIZE} fill={*color}>{part}</text>
                        ));
                    }
                }
                left -= take;
                col += take;
                if take < n {
                    break;
                }
            }
            if left > 0 || col > 0 {
                caret = (CODE_X + col as f32 * ADV, y);
            }
            rows.push(fframes::svgr!(<g>{spans}</g>));
            if left == 0 {
                break;
            }
        }
        let dim = 1.0 - 0.75 * prog(lb, 12.0, 12.3);
        let out = output(&frame, lb);
        let words = big_words(lb);
        let caret_on = (lb * 2.0).fract() < 0.6;

        fframes::svgr!(
            <g>
                <g opacity={dim}>
                    <text x={CODE_X} y="220" font-family={MONO} font-weight="500" font-size="22" letter-spacing="3" fill={GREY}>"SRC/LIB.RS"</text>
                    <rect x={CODE_X} y="236" width="1000" height="1" fill={GREY} opacity="0.5" />
                    {rows}
                    <rect x={caret.0 + 2.0} y={caret.1 - 28.0} width="16" height="36" fill={ORANGE} opacity={if caret_on { 1.0 } else { 0.0 }} />
                    {callout(CODE_X + 13.0 * ADV, CODE_Y + 3.0 * LINE_H - 40.0, CODE_X + 13.0 * ADV, 190.0, "CALLED FOR EVERY FRAME".to_owned(), ORANGE, prog(lb, 8.0, 8.8))}
                    {callout(CODE_X + 13.0 * ADV, CODE_Y + 5.0 * LINE_H + 12.0, 520.0, 790.0, "RETURNS AN SVG TREE".to_owned(), BONE, prog(lb, 9.0, 9.8))}
                </g>
                {out}
                {words}
            </g>
        )
    }
}

/// The frame the code draws: EVERY FRAME, bouncing on every beat.
fn output(frame: &Frame, lb: f32) -> Svgr<'static> {
    let appear = expo_out(prog(lb, 3.0, 4.0));
    if appear <= 0.0 {
        return Svgr::empty();
    }
    let exit = expo_in(prog(lb, 11.7, 12.0));
    let (x, y, w, h) = (1190.0, 320.0, 600.0, 338.0);
    let beat_phase = lb.fract();
    let bounce = spring(beat_phase * BEAT, 260.0, 14.0);
    let ty = (1.0 - bounce) * -60.0;
    let fnum = format!("FRAME {:05}", frame.global_index);
    fframes::svgr!(
        <g opacity={appear * (1.0 - exit)} transform={format!("translate({} 0)", (1.0 - appear) * 120.0)}>
            <rect x={x} y={y} width={w} height={h} fill="#141312" stroke="#3a3733" stroke-width="1.5" />
            {corners(x - 14.0, y - 14.0, w + 28.0, h + 28.0, 18.0, ORANGE, 2.0)}
            <clipPath id="code-out"><rect x={x} y={y} width={w} height={h} /></clipPath>
            <g clip-path="url(#code-out)">
                <text x={x + w / 2.0} y={y + h / 2.0 + 30.0 + ty} text-anchor="middle" font-family={DISPLAY} font-size="76" letter-spacing="-2" fill={BONE}>"EVERY FRAME"</text>
            </g>
            {label(x, y + h + 44.0, "OUTPUT".to_owned(), GREY, 18.0, "start")}
            {label(x + w, y + h + 44.0, fnum, ORANGE, 18.0, "end")}
            {callout(x + w / 2.0, y + h + 14.0, x + w / 2.0 - 40.0, 820.0, "DRAWN BY SKIA ON THE GPU".to_owned(), ORANGE, prog(lb, 10.0, 10.8))}
        </g>
    )
}

/// Beats 12-14: EVERY / FRAME. over the dimmed code.
fn big_words(lb: f32) -> Svgr<'static> {
    let l = lb - 12.0;
    if l < 0.0 {
        return Svgr::empty();
    }
    fframes::svgr!(
        <g>
            {Slam::new(150.0, 560.0, "EVERY", DISPLAY, 250.0, BONE).draw(l, 0.0, 190.0)}
            {Slam::new(150.0, 800.0, "FRAME.", DISPLAY, 250.0, ORANGE).draw(l - 1.0, 0.0, 190.0)}
        </g>
    )
}

/// The bar that goes (nearly) silent before the drop.
fn silence(l: f32) -> Svgr<'static> {
    let o = prog(l, 0.0, 0.2);
    let w = snap(l - 0.9);
    fframes::svgr!(
        <g>
            <rect width="1920" height="1080" fill={BG} />
            <text x="960" y="560" text-anchor="middle" font-family={MONO} font-weight="500" font-size="40" letter-spacing="12" fill={GREY} opacity={o}>"RENDERED ON THE"</text>
            <rect x={960.0 - 30.0 * w} y="600" width={(60.0 * w).max(0.5)} height="4" fill={ORANGE} />
        </g>
    )
}
