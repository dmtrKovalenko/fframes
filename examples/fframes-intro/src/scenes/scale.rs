//! The riser asks HOW FAST? and then draws one frame of the benchmark with
//! its 3,334 text nodes; 300 such frames make a million.

use fframes::{Duration, FFramesContext, Frame, Scene, Svgr};

use crate::beat::*;
use crate::ui::*;

beat_scene!(ScaleScene, Some(80.0), Some(96.0));

pub const NODES_PER_FRAME: usize = 3_334;
const COLS: usize = 62;
const ROWS: usize = 54;
const WORDS: &[&str] = &["ff", "fx", "0x", "60", "rs", "gp", "sk", "vk", "mt", "tx"];

impl Scene for ScaleScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        if lb >= 14.0 {
            return countdown(lb - 14.0);
        }
        let q = question(lb);
        let wall = wall(lb);
        fframes::svgr!(<g>{wall}{q}</g>)
    }
}

/// HOW / FAST? on beats 0 and 1, then it shrinks into a heading.
fn question(lb: f32) -> Svgr<'static> {
    let shrink = cubic_in_out(prog(lb, 3.0, 4.0));
    let s = lerp(1.0, 0.28, shrink);
    let t = format!(
        "translate({} {}) scale({s})",
        lerp(0.0, 150.0 - 150.0 * s, shrink),
        lerp(0.0, 34.0, shrink)
    );
    fframes::svgr!(
        <g transform={t}>
            {Slam::new(150.0, 440.0, "HOW", DISPLAY, 330.0, BONE).draw(lb, -260.0, 0.0)}
            {Slam::new(150.0, 760.0, "FAST?", DISPLAY, 330.0, ORANGE).draw(lb - 1.0, 260.0, 0.0)}
        </g>
    )
}

/// 3,334 tiny text nodes filling in with a diagonal wave.
fn wall(lb: f32) -> Svgr<'static> {
    let l = lb - 4.0;
    if l < 0.0 {
        return Svgr::empty();
    }
    let fill = cubic_in_out(prog(l, 0.0, 8.0));
    let x0 = 150.0;
    let y0 = 345.0;
    let dx = (1770.0 - x0) / COLS as f32;
    let dy = (900.0 - y0) / ROWS as f32;
    let mut nodes = Vec::with_capacity(NODES_PER_FRAME);
    let mut shown = 0usize;
    for i in 0..NODES_PER_FRAME {
        let c = i % COLS;
        let r = i / COLS;
        let key = (c as f32 / COLS as f32) * 0.7 + (r as f32 / ROWS as f32) * 0.3;
        if key > fill * 1.02 {
            continue;
        }
        shown += 1;
        let h = hash(i as f32 * 1.37 + (lb * 4.0).floor());
        let word = WORDS[(hash(i as f32) * WORDS.len() as f32) as usize % WORDS.len()];
        let fresh = fill * 1.02 - key < 0.03;
        let color = if fresh || h > 0.97 {
            ORANGE
        } else if h > 0.55 {
            "#b3aca2"
        } else {
            "#6b665f"
        };
        let x = x0 + c as f32 * dx;
        let y = y0 + r as f32 * dy;
        nodes.push(fframes::svgr!(<text x={x} y={y} font-family={MONO} font-size="13" font-weight="500" fill={color}>{word}</text>));
    }
    let count = (shown as f32 / NODES_PER_FRAME as f32 * 1_000_000.0).round() as u64;
    let exit = expo_in(prog(l, 9.6, 10.0));
    fframes::svgr!(
        <g opacity={1.0 - exit}>
            {nodes}
            <rect x="1010" y="130" width="760" height="140" fill={BG} />
            <text x="1770" y="232" text-anchor="end" font-family={DISPLAY} font-size="110" letter-spacing="-3" fill={BONE}>{thousands(count)}</text>
            {label(1770.0, 290.0, "TEXT NODES TO RENDER".to_owned(), ORANGE, 20.0, "end")}
            {label(150.0, 940.0, format!("ONE FRAME = {} TEXT NODES  ×  300 FRAMES", thousands(NODES_PER_FRAME as u64)), GREY, 20.0, "start")}
        </g>
    )
}

/// The near silent bar before the drop: two names, then nothing.
fn countdown(l: f32) -> Svgr<'static> {
    let a = snap(l);
    let v = prog(l, 0.5, 0.55);
    let b = snap(l - 0.5);
    let out = prog(l, 1.55, 1.6);
    fframes::svgr!(
        <g opacity={1.0 - out}>
            <text x="960" y={470.0 + (1.0 - a) * 40.0} text-anchor="middle" font-family={DISPLAY} font-size="120" letter-spacing="-3" fill={GREY}>"REMOTION"</text>
            <text x="960" y="560" text-anchor="middle" font-family={SERIF} font-style="italic" font-size="70" fill={BONE} opacity={v}>"vs"</text>
            <text x="960" y={690.0 + (1.0 - b) * 40.0} text-anchor="middle" font-family={DISPLAY} font-size="120" letter-spacing="-3" fill={ORANGE} opacity={v}>"FFRAMES"</text>
        </g>
    )
}
