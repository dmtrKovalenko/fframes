//! The second drop: frames of this video fly out of the grid, next to the
//! time it took to render all of them.

use fframes::{Color, Duration, FFramesContext, Frame, Scene, ShaderUniforms, Svgr};

use crate::beat::*;
use crate::facts;
use crate::shaders::SHADERS;
use crate::ui::*;

beat_scene!(RenderScene, Some(208.0), Some(224.0));

const THUMBS: usize = 10;

impl Scene for RenderScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        let grid = SHADERS.grid.draw(
            &frame,
            ShaderUniforms::new()
                .float("uSpeed", 6.0)
                .float("uHorizon", 0.5)
                .float("uBright", 0.9 + pulse(lb, 6.0) * 0.4)
                .color("uInk", Color::hex("#6f6a63"))
                .color("uHot", Color::hex(ORANGE)),
        );

        // thumbnails on a stream towards the camera, far ones first
        let t = lb * BEAT;
        let mut cards: Vec<(f32, Svgr)> = Vec::new();
        for k in 0..14 {
            let lane = k % THUMBS;
            let phase = (t * 0.45 + k as f32 / 14.0).fract();
            let z = lerp(7.0, 0.35, phase);
            let side = if k % 2 == 0 { -1.0 } else { 1.0 };
            let xw = side * (620.0 + hash(k as f32 * 3.1) * 520.0);
            let yw = -330.0 + hash(k as f32 * 7.7) * 300.0;
            let x = 960.0 + xw / z;
            let y = 540.0 + yw / z;
            let w = 520.0 / z;
            let h = w * 9.0 / 16.0;
            let o = prog(phase, 0.0, 0.2) * (1.0 - prog(phase, 0.85, 1.0)) * prog(lb, 0.0, 0.3);
            let name = format!("fly_{lane}.jpg");
            if let Some(img) = ctx.get_image(&name) {
                cards.push((
                    z,
                    fframes::svgr!(
                        <g opacity={o}>
                            <image href={img.href()} x={x - w / 2.0} y={y - h / 2.0} width={w} height={h} />
                            <rect x={x - w / 2.0} y={y - h / 2.0} width={w} height={h} fill="none" stroke={BONE} stroke-opacity="0.35" stroke-width="1" />
                        </g>
                    ),
                ));
            }
        }
        cards.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let cards: Vec<Svgr> = cards.into_iter().map(|(_, c)| c).collect();

        // letter-spacing -4 over 11 glyphs
        let title_w = measure(&mut frame, ctx, DISPLAY, 120, 400, false, "THIS VIDEO.") - 44.0;
        let count = (facts::FRAMES as f32 * expo_out(prog(lb, 0.0, 3.0))).round() as u64;
        let secs = facts::RENDER_SECONDS * expo_out(prog(lb, 4.0, 6.0));
        let a = snap(lb);
        let b = snap(lb - 4.0);
        let realtime = if facts::RENDER_SECONDS > 0.0 {
            crate::beat::TOTAL_SECONDS / facts::RENDER_SECONDS
        } else {
            0.0
        };
        let c = prog(lb, 8.0, 8.3);
        let exit = expo_in(prog(lb, 15.7, 16.0));

        fframes::svgr!(
            <g opacity={1.0 - exit}>
                <image href={grid.href()} x="0" y="0" width="1920" height="1080" />
                {cards}
                <rect x="0" y="600" width="1920" height="480" fill="url(#render-fade)" />
                <defs>
                    <linearGradient id="render-fade" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="0" stop-color="#0b0b0b" stop-opacity="0" />
                        <stop offset="1" stop-color="#0b0b0b" stop-opacity="0.9" />
                    </linearGradient>
                </defs>
                <g transform={format!("translate({} 0)", (1.0 - snap(lb - 0.5)) * -200.0)} opacity={prog(lb, 0.5, 0.6)}>
                    <rect x="120" y="120" width={title_w + 60.0} height="150" fill={ORANGE} />
                    <text x="150" y="232" font-family={DISPLAY} font-size="120" letter-spacing="-4" fill={INK}>"THIS VIDEO."</text>
                </g>
                <g transform={format!("translate(0 {})", (1.0 - a) * 70.0)} opacity={prog(lb, 0.0, 0.08)}>
                    <text x="150" y="820" font-family={DISPLAY} font-size="190" letter-spacing="-8" fill={BONE}>{thousands(count)}</text>
                    <text x="160" y="880" font-family={MONO} font-weight="600" font-size="30" letter-spacing="6" fill={BONE}>"FRAMES · 1920×1080 · 60 FPS"</text>
                </g>
                <g transform={format!("translate(0 {})", (1.0 - b) * 70.0)} opacity={prog(lb, 4.0, 4.08)}>
                    <text x="1770" y="820" text-anchor="end" font-family={DISPLAY} font-size="190" letter-spacing="-8" fill={ORANGE}>{format!("{secs:.1}s")}</text>
                    <text x="1770" y="880" text-anchor="end" font-family={MONO} font-weight="600" font-size="30" letter-spacing="6" fill={BONE}>"TO RENDER · SKIA ON METAL"</text>
                </g>
                <g opacity={c}>
                    <rect x="1330" y="170" width="440" height="64" fill={ORANGE} />
                    <text x="1550" y="214" text-anchor="middle" font-family={MONO} font-weight="700" font-size="30" letter-spacing="3" fill={INK}>{format!("{realtime:.1}× REAL TIME")}</text>
                </g>
            </g>
        )
    }
}
