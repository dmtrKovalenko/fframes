//! A wall of marquee rows; the lit row jumps on every beat.

use fframes::{Duration, FFramesContext, Frame, Scene, Svgr};

use crate::beat::*;
use crate::ui::*;

beat_scene!(TextScene, Some(56.0), Some(62.0));

const ROWS: &[&str] = &[
    "TEXT · SHAPED · MEASURED · WRAPPED · ",
    "KERNING · LIGATURES · 60 FPS · ",
    "EVERY GLYPH ON THE GPU · ",
    "TEXT · TEXT · TEXT · TEXT · ",
    "FONTS COMPILED INTO THE BINARY · ",
    "TYPOGRAPHY IS CODE · ",
    "TEXT · SHAPED · MEASURED · ",
    "WORD BY WORD · LINE BY LINE · ",
];

const ROW_H: f32 = 140.0;
const SIZE: usize = 160;

impl Scene for TextScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        let exit = expo_in(prog(lb, 5.7, 6.0));
        let lit = (lb.max(0.0).floor() as usize * 3 + 1) % ROWS.len();
        let mut rows = Vec::new();
        for (i, text) in ROWS.iter().enumerate() {
            let w = measure(&mut frame, ctx, COND, SIZE, 400, false, text);
            let dir = if i % 2 == 0 { -1.0 } else { 1.0 };
            let speed = 260.0 + (i as f32 * 37.0) % 140.0;
            let enter = expo_out(prog(lb, i as f32 * 0.05, 0.6 + i as f32 * 0.05));
            let offset = (lb * BEAT * speed * dir).rem_euclid(w) - w + (1.0 - enter) * 900.0 * dir;
            let y = 150.0 + i as f32 * ROW_H;
            let is_lit = i == lit;
            let flash = if is_lit { pulse(lb, 5.0) } else { 0.0 };
            let copies: Vec<Svgr> = (0..4)
                .map(|k| {
                    let x = offset + k as f32 * w;
                    let t = (*text).to_owned();
                    if is_lit {
                        fframes::svgr!(<text x={x} y={y} font-family={COND} font-size={SIZE} fill={ORANGE}>{t}</text>)
                    } else {
                        fframes::svgr!(<text x={x} y={y} font-family={COND} font-size={SIZE} fill="none" stroke="#4b4741" stroke-width="1.6">{t}</text>)
                    }
                })
                .collect();
            rows.push(fframes::svgr!(<g opacity={0.7 + flash * 0.3}>{copies}</g>));
        }
        fframes::svgr!(
            <g opacity={1.0 - exit}>
                {rows}
                <rect x="120" y="360" width="690" height="270" fill={BG} />
                <text x="150" y="560" font-family={DISPLAY} font-size="170" letter-spacing="-6" fill={BONE}>"TEXT"</text>
                {label(154.0, 408.0, "01".to_owned(), ORANGE, 26.0, "start")}
                {label(154.0, 610.0, "SHAPED · MEASURED · WRAPPED".to_owned(), GREY, 18.0, "start")}
            </g>
        )
    }
}
