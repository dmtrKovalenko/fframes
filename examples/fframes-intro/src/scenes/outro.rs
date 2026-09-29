//! The fframes wordmark slams in with its trail of f's, followed by the
//! install commands.

use fframes::{Color, Duration, FFramesContext, Frame, Scene, ShaderUniforms, Svgr};

use crate::beat::*;
use crate::shaders::SHADERS;
use crate::ui::*;

beat_scene!(OutroScene, Some(240.0), None);

const WORD_SIZE: usize = 300;

impl Scene for OutroScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        let t_end = crate::beat::TOTAL_SECONDS - gsec(&frame);
        let fade_out = prog(1.6 - t_end, 0.0, 1.6);
        let flash = (-lb * 9.0).exp();

        let bg = SHADERS.contour.draw(
            &frame,
            ShaderUniforms::new()
                .float2("uWell", 0.5, 0.47)
                .float("uDepth", 1.1)
                .float("uDensity", 18.0)
                .float("uBright", 0.25 + 0.5 * (1.0 - expo_out(prog(lb, 0.0, 6.0))))
                .color("uInk", Color::hex("#8a857c"))
                .color("uHot", Color::hex(ORANGE)),
        );

        let w = measure(&mut frame, ctx, SERIF, WORD_SIZE, 400, true, "fframes");
        let fw = measure(&mut frame, ctx, SERIF, WORD_SIZE, 400, true, "f");
        // the trail makes the whole mark wider: center mark + trail
        let trail = 3.0;
        let step = fw * 0.78;
        let total = w + step * trail;
        let x0 = 960.0 - total / 2.0 + step * trail;
        let y = 520.0;
        let word_in = spring(lb * BEAT, 260.0, 20.0);
        let word_s = 1.0 + (1.0 - expo_out(lb / 0.5)) * 0.25;

        let echoes: Vec<Svgr> = (0..3)
            .map(|k| {
                let k = k as f32;
                let s = soft(lb - 0.5 - k * 0.5);
                let dx = -step * (k + 1.0) * s;
                let colors = [ORANGE, "#c4531c", EMBER];
                let color = colors[k as usize];
                let o = s.clamp(0.0, 1.0) * (1.0 - k * 0.18);
                fframes::svgr!(
                    <text x={x0 + dx} y={y} font-family={SERIF} font-style="italic" font-size={WORD_SIZE} fill={color} opacity={o}>"f"</text>
                )
            })
            .rev()
            .collect();

        let tag = prog(lb, 4.0, 4.6);
        let cmds = [
            (
                "npx skills add https://fframes.studio",
                "WITH YOUR CODING AGENT",
            ),
            ("cargo fframes new my-video", "OR BY HAND"),
        ];
        let cmd_rows: Vec<Svgr> = cmds
            .iter()
            .enumerate()
            .map(|(i, (cmd, note))| {
                let at = 8.0 + i as f32 * 1.0;
                let s = snap(lb - at);
                let y = 760.0 + i as f32 * 78.0;
                fframes::svgr!(
                    <g opacity={prog(lb, at, at + 0.1)} transform={format!("translate(0 {})", (1.0 - s) * 30.0)}>
                        <rect x="520" y={y - 44.0} width="880" height="62" fill="#0f0e0d" fill-opacity="0.9" stroke="#3d3a35" stroke-width="1.5" />
                        <text x="548" y={y} font-family={MONO} font-weight="600" font-size="26" fill={ORANGE}>"$"</text>
                        <text x="578" y={y} font-family={MONO} font-weight="500" font-size="26" fill={BONE}>{*cmd}</text>
                        <text x="1380" y={y} text-anchor="end" font-family={MONO} font-weight="500" font-size="15" letter-spacing="2" fill={GREY}>{*note}</text>
                    </g>
                )
            })
            .collect();
        let url = prog(lb, 12.0, 12.5);

        fframes::svgr!(
            <g>
                <image href={bg.href()} x="0" y="0" width="1920" height="1080" />
                <g opacity={1.0 - fade_out}>
                    {echoes}
                    <g transform={format!("translate(960 {y}) scale({word_s}) translate(-960 -{y})")} opacity={word_in.min(1.0)}>
                        <text x={x0} y={y} font-family={SERIF} font-style="italic" font-size={WORD_SIZE} fill={BONE}>"fframes"</text>
                    </g>
                    <text x="960" y="640" text-anchor="middle" font-family={MONO} font-weight="500" font-size="30" letter-spacing="2" fill={BONE} opacity={tag}>"video vibe coding framework that is actually fast"</text>
                    {cmd_rows}
                    <g opacity={url}>
                        <text x="960" y="990" text-anchor="middle" font-family={MONO} font-weight="600" font-size="26" letter-spacing="3" fill={ORANGE}>"GITHUB.COM/DMTRKOVALENKO/FFRAMES"</text>
                    </g>
                </g>
                <rect width="1920" height="1080" fill={BONE} opacity={flash} />
                <rect width="1920" height="1080" fill="#000000" opacity={fade_out} />
            </g>
        )
    }
}
