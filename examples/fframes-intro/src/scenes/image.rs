//! The 2024 beta poster develops from an orange dither.

use fframes::{Color, Duration, FFramesContext, Frame, Scene, ShaderUniforms, Svgr};

use crate::beat::*;
use crate::shaders::SHADERS;
use crate::ui::*;

beat_scene!(ImageScene, Some(62.0), Some(66.0));

impl Scene for ImageScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        let Some(poster) = ctx.get_image("beta_poster.jpg") else {
            return Svgr::empty();
        };
        let (x, y, w, h) = (560.0, 300.0, 1060.0, 596.0);
        let enter = expo_out(prog(lb, 0.0, 0.6));
        let exit = expo_in(prog(lb, 3.7, 4.0));
        let layer = SHADERS.develop.draw(
            &frame,
            ShaderUniforms::new()
                .image("iChannel0", poster)
                .float2("uImg", 1920.0, 1080.0)
                .float("uReveal", prog(lb, 0.0, 0.7))
                .float("uProgress", cubic_in_out(prog(lb, 0.9, 3.1)) * 1.02)
                .color("uInk", Color::hex("#1a1210"))
                .color("uHot", Color::hex(ORANGE)),
        );
        let s = 0.94 + 0.06 * enter;
        let t = format!("translate(960 {}) scale({s}) translate(-960 -{})", y + h / 2.0, y + h / 2.0);
        fframes::svgr!(
            <g opacity={1.0 - exit}>
                <g transform={t}>
                    <image href={layer.href()} x={x} y={y} width={w} height={h} />
                    {corners(x - 18.0, y - 18.0, w + 36.0, h + 36.0, 22.0, BONE, 2.0)}
                </g>
                <text x="150" y="220" font-family={DISPLAY} font-size="120" letter-spacing="-4" fill={BONE} opacity={prog(lb, 0.2, 0.3)}>"IMAGE"</text>
                {label(154.0, 110.0, "02".to_owned(), ORANGE, 26.0, "start")}
                {label(x, y + h + 60.0, "BETA_POSTER.JPG · 2024".to_owned(), GREY, 18.0, "start")}
                {label(x + w, y + h + 60.0, "DITHER → COLOR · ONE SKSL PASS".to_owned(), ORANGE, 18.0, "end")}
            </g>
        )
    }
}
