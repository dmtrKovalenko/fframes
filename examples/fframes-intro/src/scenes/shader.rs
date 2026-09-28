//! The tunnel that showed through the keyed feed grows to the whole frame,
//! its SkSL source scrolls on the left and the guest becomes a halftone print.

use fframes::{Duration, FFramesContext, Frame, Scene, Svgr};

use crate::beat::*;
use crate::scenes::video::{RIGHT, keyed_guest, tunnel};
use crate::shaders::TUNNEL_SOURCE;
use crate::ui::*;

beat_scene!(ShaderScene, Some(72.0), Some(80.0));

impl Scene for ShaderScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        // the right feed's frame grows to the full canvas
        let open = expo_out(prog(lb, 0.0, 1.0));
        let (rx, ry, rw, rh) = RIGHT;
        let cx = lerp(rx, 0.0, open);
        let cy = lerp(ry, 0.0, open);
        let cw = lerp(rw, 1920.0, open);
        let ch = lerp(rh, 1080.0, open);
        // the guest moves from the feed to the right half and prints as halftone
        let gx = lerp(rx, 860.0, open);
        let gy = lerp(ry, 150.0, open);
        let gw = lerp(rw, 1280.0, open);
        let gh = lerp(rh, 720.0, open);
        let halftone = prog(lb, 0.8, 1.6);

        let lines: Vec<Svgr> = TUNNEL_SOURCE
            .lines()
            .filter(|l| !l.trim().is_empty())
            .enumerate()
            .map(|(i, l)| {
                let y = 300.0 + i as f32 * 26.0 - lb * 40.0;
                let text: String = l.chars().take(60).collect();
                let o = if (250.0..900.0).contains(&y) { 1.0 } else { 0.0 };
                fframes::svgr!(<text x="150" y={y} font-family={MONO} font-size="17" fill="#d8d0c4" opacity={o * 0.85}>{text}</text>)
            })
            .collect();
        let panel = prog(lb, 1.0, 1.6);
        let title = snap(lb - 0.8);
        let all = snap(lb - 4.0);
        let exit = expo_in(prog(lb, 7.7, 8.0));

        fframes::svgr!(
            <g opacity={1.0 - exit}>
                <clipPath id="shader-open"><rect x={cx} y={cy} width={cw.max(1.0)} height={ch.max(1.0)} /></clipPath>
                <g clip-path="url(#shader-open)">{tunnel(&frame, 3.0, 1.0)}</g>
                {keyed_guest(&frame, ctx, (gx, gy, gw, gh), halftone, 1.0)}
                <g opacity={panel}>
                    <rect x="120" y="250" width="720" height="650" fill="#0b0b0b" fill-opacity="0.72" />
                    <clipPath id="shader-src"><rect x="120" y="250" width="720" height="650" /></clipPath>
                    <g clip-path="url(#shader-src)">{lines}</g>
                    {corners(120.0, 250.0, 720.0, 650.0, 18.0, ORANGE, 2.0)}
                    {label(140.0, 935.0, "TUNNEL.SKSL · RAYMARCHED · 80 STEPS/PIXEL".to_owned(), ORANGE, 18.0, "start")}
                </g>
                <g opacity={prog(lb, 0.8, 0.9)} transform={format!("translate(0 {})", (1.0 - title) * 50.0)}>
                    <text x="150" y="220" font-family={DISPLAY} font-size="120" letter-spacing="-4" fill={BONE}>"SHADER"</text>
                    {label(154.0, 110.0, "04".to_owned(), ORANGE, 26.0, "start")}
                </g>
                <g opacity={prog(lb, 4.0, 4.1)} transform={format!("translate(0 {})", (1.0 - all) * 60.0)}>
                    <rect x="860" y="850" width="930" height="120" fill={ORANGE} />
                    <text x="890" y="940" font-family={DISPLAY} font-size="74" letter-spacing="-2" fill={INK}>"ALL IN ONE FRAME."</text>
                </g>
            </g>
        )
    }
}
