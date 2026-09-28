//! The silent bar: only a command being typed.

use fframes::{Duration, FFramesContext, Frame, Scene, Svgr};

use crate::beat::*;
use crate::ui::*;

beat_scene!(DoneScene, Some(196.0), Some(200.0));

const COMMAND: &str = "cargo fframes new my-video";

impl Scene for DoneScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        let n = COMMAND.len() as f32;
        let typed = (prog(lb, 0.2, 3.2) * n) as usize;
        let shown: String = COMMAND.chars().take(typed).collect();
        // text is centered on the full command so it does not shift while typing
        let adv = 40.0 * 0.6;
        let x0 = 960.0 - (n + 2.0) * adv / 2.0;
        let caret_x = x0 + (2.0 + typed as f32) * adv;
        let caret_on = typed as f32 >= n && (lb * 2.0).fract() > 0.5 || (typed as f32) < n;
        fframes::svgr!(
            <g>
                <rect width="1920" height="1080" fill="#050505" />
                <text x={x0} y="555" font-family={MONO} font-weight="600" font-size="40" fill={ORANGE}>"$"</text>
                <text x={x0 + 2.0 * adv} y="555" font-family={MONO} font-weight="500" font-size="40" fill={BONE}>{shown}</text>
                <rect x={caret_x + 2.0} y="522" width="20" height="44" fill={ORANGE} opacity={if caret_on { 1.0 } else { 0.0 }} />
            </g>
        )
    }
}
