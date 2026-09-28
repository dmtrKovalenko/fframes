//! One word per beat on alternating full-bleed colors.

use fframes::{Duration, FFramesContext, Frame, Scene, Svgr};

use crate::beat::*;
use crate::ui::*;

beat_scene!(RecapScene, Some(184.0), Some(196.0));

const WORDS: &[&str] = &["TEXT", "IMAGES", "VIDEO", "SHADERS", "AUDIO", "GPU", "EFFECTS", "AGENTS"];

impl Scene for RecapScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, _frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&_frame);
        let i = lb.max(0.0).floor() as usize;
        let local = lb - i as f32;
        if i < WORDS.len() {
            let (bg, fg) = match i % 3 {
                0 => (ORANGE, INK),
                1 => (BG, BONE),
                _ => (PAPER, INK),
            };
            let punch = 1.0 + (1.0 - expo_out(local / 0.4)) * 0.12;
            let n = format!("{:02}", i + 1);
            let word = WORDS[i];
            return fframes::svgr!(
                <g>
                    <rect width="1920" height="1080" fill={bg} />
                    <g transform={format!("translate(960 560) scale({punch}) translate(-960 -560)")}>
                        <text x="960" y="690" text-anchor="middle" font-family={DISPLAY} font-size="360" letter-spacing="-16" fill={fg}>{word}</text>
                    </g>
                    <text x="150" y="200" font-family={MONO} font-weight="600" font-size="30" letter-spacing="6" fill={fg}>{n}</text>
                    <text x="1770" y="200" text-anchor="end" font-family={MONO} font-weight="600" font-size="30" letter-spacing="6" fill={fg}>"/ 08"</text>
                </g>
            );
        }
        // beats 8-12: ALL OF IT. / in Rust.
        let l = lb - 8.0;
        let exit = expo_in(prog(l, 3.7, 4.0));
        fframes::svgr!(
            <g opacity={1.0 - exit}>
                <rect width="1920" height="1080" fill={BG} />
                {Slam::new(960.0, 520.0, "ALL OF IT.", DISPLAY, 220.0, BONE).anchor("middle").draw(l, 0.0, 170.0)}
                {Slam::new(960.0, 760.0, "in Rust.", SERIF, 220.0, ORANGE).anchor("middle").italic().draw(l - 2.0, 0.0, 170.0)}
            </g>
        )
    }
}
