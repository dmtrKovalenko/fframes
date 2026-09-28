//! Cold open: the prompt that asked for this video, typed into a coding agent.

use fframes::{Duration, FFramesContext, Frame, Scene, ShaderUniforms, Svgr};

use crate::beat::*;
use crate::shaders::SHADERS;
use crate::ui::*;

beat_scene!(PromptScene, None, Some(0.0));

pub const PROMPT: &str = "make an intro video for fframes. make it stunning.";

/// Agent output after the prompt: (is a tool call, text).
const OUTPUT: &[(bool, &str)] = &[
    (true, "Explored skills/fframes-video"),
    (false, "Read SKILL.md, design.md, audio.md"),
    (true, "Edited src/lib.rs"),
    (true, "Ran cargo run --release -- timeline"),
    (true, "Ran cargo run --release -- strip all -n 24"),
];

impl Scene for PromptScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = gsec(&frame);
        let enter_at = beat_time(-1.0);
        let chars = PROMPT.chars().count();
        let typed = ((8.0 + (t - 0.02) * 44.0) as usize).min(chars);
        let shown: String = PROMPT.chars().take(typed).collect();
        let done = typed >= chars;
        // caret blinks once typing is done
        let caret_on = !done || (t * 3.0).fract() < 0.55;
        // two lines: the first sentence, then "make it stunning."
        let split = PROMPT.find(". ").map(|i| i + 2).unwrap_or(chars);
        let line1: String = shown.chars().take(split).collect();
        let line2: String = shown.chars().skip(split).collect();
        let adv = 64.0 * 0.6;
        let on_second = typed > split;
        let caret_x = 240.0 + if on_second { (typed - split) as f32 } else { typed as f32 } * adv;
        let caret_y = if on_second { 612.0 } else { 522.0 };

        let after = t - enter_at;
        let lift = if after > 0.0 { -expo_out(after / 0.25) * 200.0 } else { 0.0 };
        let enter_flash = if after > 0.0 { (-after * 9.0).exp() } else { 0.0 };

        let lines: Vec<Svgr> = OUTPUT
            .iter()
            .enumerate()
            .map(|(i, (call, text))| {
                let at = enter_at + 0.05 + i as f32 * 0.07;
                let o = prog(t, at, at + 0.06);
                let y = 560.0 + i as f32 * 44.0;
                let marker = if *call {
                    fframes::svgr!(<circle cx="172" cy={y - 9.0} r="7" fill={ORANGE} />)
                } else {
                    fframes::svgr!(<path d={format!("M204 {} V{} H222", y - 26.0, y - 9.0)} fill="none" stroke={GREY} stroke-width="2" />)
                };
                let x = if *call { 200.0 } else { 236.0 };
                fframes::svgr!(
                    <g opacity={o}>
                        {marker}
                        <text x={x} y={y} font-family={MONO} font-size="28" fill="#b9b4ab">{*text}</text>
                    </g>
                )
            })
            .collect();

        let bg = SHADERS.contour.draw(
            &frame,
            ShaderUniforms::new()
                .float2("uWell", 0.74, 0.62)
                .float("uDepth", 0.6)
                .float("uDensity", 16.0)
                .float("uBright", 0.35 + enter_flash * 0.4)
                .color("uInk", fframes::Color::hex("#8a857c"))
                .color("uHot", fframes::Color::hex(ORANGE)),
        );

        fframes::svgr!(
            <g>
                <image href={bg.href()} x="0" y="0" width="1920" height="1080" />
                <g transform={format!("translate(0 {lift})")}>
                    <text x="160" y="366" font-family={MONO} font-weight="500" font-size="22" letter-spacing="2" fill={GREY}>"CODEX  ·  ~/dev/fframes"</text>
                    <rect x="140" y="400" width="1640" height="270" rx="18" fill="#0f0e0d" fill-opacity="0.85"
                          stroke={if after > 0.0 { ORANGE } else { "#4a4640" }} stroke-width="2" />
                    <text x="180" y="522" font-family={MONO} font-weight="600" font-size="64" fill={ORANGE}>"›"</text>
                    <text x="240" y="522" font-family={MONO} font-weight="500" font-size="64" fill={BONE}>{line1}</text>
                    <text x="240" y="612" font-family={MONO} font-weight="500" font-size="64" fill={ORANGE}>{line2}</text>
                    <rect x={caret_x + 4.0} y={caret_y - 50.0} width="32" height="64" fill={ORANGE} opacity={if caret_on && after <= 0.0 { 1.0 } else { 0.0 }} />
                </g>
                <g transform="translate(0 -40)">{lines}</g>
            </g>
        )
    }
}
