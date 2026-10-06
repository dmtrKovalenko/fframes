//! Vector wordmark animation from `fframes-intro/src/scenes/outro.rs`.
//! Keep the intro's font, spring parameters, stagger and orange echo palette.

use std::sync::OnceLock;

use fframes::{FFramesContext, FontQuery, FontStyle, Frame, Svgr};

const FONT: &str = "Instrument Serif";
const WORD_SIZE: usize = 300;
const BEAT: f32 = 0.454_522;

#[derive(Debug, Default)]
pub(crate) struct Outro {
    widths: OnceLock<(f32, f32)>,
}

impl Outro {
    fn measure(&self, frame: &mut Frame, ctx: &FFramesContext<'_, '_>) -> (f32, f32) {
        if let Some(widths) = self.widths.get() {
            return *widths;
        }
        let font = FontQuery {
            family: FONT,
            size: WORD_SIZE,
            weight: 400,
            style: FontStyle::Italic,
            ..Default::default()
        };
        match (
            frame.text_width(ctx, font, "fframes"),
            frame.text_width(ctx, font, "f"),
        ) {
            (Some(word), Some(letter)) => *self.widths.get_or_init(|| (word as f32, letter as f32)),
            // Do not cache a fallback if fonts are temporarily unavailable.
            _ => (840.0, 84.0),
        }
    }

    pub(crate) fn render(&self, frame: &mut Frame, ctx: &FFramesContext<'_, '_>) -> Svgr<'static> {
        let seconds = frame.seconds();
        let beats = seconds / BEAT;
        let (width, letter_width) = self.measure(frame, ctx);
        let step = letter_width * 0.78;
        let total = width + step * 3.0;
        let x = 960.0 - total / 2.0 + step * 3.0;
        let y = 530.0;
        let word_opacity = spring(seconds, 260.0, 20.0).min(1.0);
        let word_scale = 1.0 + (1.0 - expo_out(beats / 0.5)) * 0.25;
        let echoes: Vec<Svgr> = ["#fb6a22", "#c4531c", "#8a3712"]
            .into_iter()
            .enumerate()
            .rev()
            .map(|(index, color)| {
                let index = index as f32;
                let amount = spring((beats - 0.5 - index * 0.5) * BEAT, 170.0, 15.0);
                let dx = -step * (index + 1.0) * amount;
                let opacity = amount.clamp(0.0, 1.0) * (1.0 - index * 0.18);
                fframes::svgr!(
                    <text x={x + dx} y={y} font-family={FONT} font-style="italic"
                          font-weight="400" font-size={WORD_SIZE} fill={color} opacity={opacity}>"f"</text>
                )
            })
            .collect();
        let tag = ((beats - 4.0) / 0.6).clamp(0.0, 1.0);

        fframes::svgr!(
            <g>
                {echoes}
                <g transform={format!("translate(960 {y}) scale({word_scale}) translate(-960 -{y})")}
                   opacity={word_opacity}>
                    <text x={x} y={y} font-family={FONT} font-style="italic"
                          font-weight="400" font-size={WORD_SIZE} fill="#ece8e1">"fframes"</text>
                </g>
                <text x="960" y="636" text-anchor="middle" font-family="JetBrains Mono"
                      font-size="30" letter-spacing="3" fill="#c6b4ef" opacity={tag}>"SHADER MODE"</text>
            </g>
        )
    }
}

// The same damped springs and exponential scale-in as fframes-intro/beat.rs.
fn spring(seconds: f32, stiffness: f32, damping: f32) -> f32 {
    if seconds <= 0.0 {
        return 0.0;
    }
    let natural = stiffness.sqrt();
    let ratio = damping / (2.0 * natural);
    let damped = natural * (1.0 - ratio * ratio).sqrt();
    1.0 - (-ratio * natural * seconds).exp()
        * ((damped * seconds).cos() + ratio * natural / damped * (damped * seconds).sin())
}

fn expo_out(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress >= 1.0 {
        1.0
    } else {
        1.0 - 2.0_f32.powf(-10.0 * progress)
    }
}
