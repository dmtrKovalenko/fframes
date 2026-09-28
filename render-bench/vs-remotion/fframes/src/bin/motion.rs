//! Workload "motion-graphics" (fframes side). Twin of ../remotion/src/MotionGraphics.tsx:
//! blurred photo backdrop with a gradient overlay, 40 animated SVG shapes, three layered
//! cards with box-shadows and blurred glows (one holds poster.jpg), and kinetic per-letter
//! headline text with a glow. Plain SVG filters, no custom shaders.
//!
//! ```sh
//! target/release/motion metal out/motion.mp4 medium          # 1920x1080, 60 fps, 10 s
//! ```

#[path = "../shared.rs"]
mod shared;

use fframes::animation::{AnimationRuntime, Easing};
use fframes::{
    AnimateRuntimeInput, AudioMap, Color, Duration, FFramesContext, FontQuery, Frame, Svgr, Video,
};
use shared::*;

struct Card {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    glow: &'static str,
}
const CARDS: [Card; 3] = [
    Card { x: 1060.0, y: 140.0, w: 700.0, h: 420.0, glow: "#a855f7" },
    Card { x: 160.0, y: 590.0, w: 560.0, h: 300.0, glow: "#22d3ee" },
    Card { x: 820.0, y: 660.0, w: 520.0, h: 240.0, glow: "#f472b6" },
];
const SHADOW_IDS: [&str; 3] = ["shadow-0", "shadow-1", "shadow-2"];
const GLOW_IDS: [&str; 3] = ["glow-0", "glow-1", "glow-2"];
const HEADLINE: [(f64, &[&str]); 2] = [(110.0, &["MOTION"]), (290.0, &["IN", "CODE"])];
/// Every letter of the headline as its own `&'static str` (text_width wants the text to live
/// as long as the frame context).
const LETTERS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

struct Motion {
    frames: usize,
    star: String,
    card_spring: AnimationRuntime,
    letter_spring: AnimationRuntime,
}

impl Video for Motion {
    const FPS: usize = 60;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(self.frames)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let mut frame = frame;
        let fps = Self::FPS as f64;
        let t = frame.index as f64 / fps;
        let spring = |frame: &Frame, on: f64, rt: &AnimationRuntime| -> f64 {
            frame.animate_runtime(AnimateRuntimeInput { on_second: on as f32, from: 0.0_f32, to: 1.0, animation_runtime: rt })
                as f64
        };
        let Some(poster) = ctx.get_image("poster.jpg") else { return Svgr::empty() };

        // 40 animated shapes
        let shapes: Vec<Svgr> = (0..SHAPES)
            .map(|i| {
                let fi = i as f64;
                let s = 24.0 + ((i * 37) % 50) as f64;
                let x = ((i * 263) % 1920) as f64 + 60.0 * (0.8 * t + fi).sin();
                let y = ((i * 149) % 1080) as f64 + 50.0 * (0.6 * t + 1.3 * fi).cos();
                let rot = t * 40.0 * if i % 2 == 1 { 1.0 } else { -1.0 } + fi * 20.0;
                let fill = hsl_to_hex((fi * 9.0 + t * 30.0) % 360.0, 0.8, 0.6);
                let tf = format!("translate({x} {y}) rotate({rot}) scale({})", s / 2.0);
                match i % 4 {
                    0 => fframes::svgr!(<circle r="1" fill={fill} opacity="0.7" transform={tf} />),
                    1 => fframes::svgr!(<rect x="-1" y="-1" width="2" height="2" rx="0.5" fill={fill} opacity="0.7" transform={tf} />),
                    2 => fframes::svgr!(<path d={TRIANGLE_PATH} fill={fill} opacity="0.7" transform={tf} />),
                    _ => fframes::svgr!(<path d={self.star.as_str()} fill={fill} opacity="0.7" transform={tf} />),
                }
            })
            .collect();

        // glows and layered cards
        let mut defs: Vec<Svgr> = Vec::new();
        let cards: Vec<Svgr> = CARDS
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let fi = i as f64;
                let p = spring(&frame, 0.4 + 0.25 * fi, &self.card_spring);
                let dy = (1.0 - p) * 120.0 + 12.0 * (1.3 * t + 2.0 * fi).sin();
                defs.push(box_shadow_filter(SHADOW_IDS[i], c.x, c.y, c.w, c.h, 40.0, 40.0, 0.45));
                defs.push(blur_filter(GLOW_IDS[i], c.x - 20.0, c.y - 20.0, c.w + 40.0, c.h + 40.0, 50.0));
                let content = match i {
                    0 => fframes::svgr!(
                        <g>
                            <clipPath id="poster-clip"><rect x={c.x} y={c.y} width={c.w} height={c.h} rx="32" /></clipPath>
                            <image href={poster.href()} x={c.x} y={c.y} width={c.w} height={c.h}
                                preserveAspectRatio="xMidYMid slice" clip-path="url(#poster-clip)" />
                        </g>
                    ),
                    1 => fframes::svgr!(
                        <g>
                            <rect x={c.x} y={c.y} width={c.w} height={c.h} rx="32" fill="url(#card-grad)" />
                            <text x={c.x + 40.0} y={baseline(c.y + 50.0, 120.0, 120.0, BEBAS.0, BEBAS.1)}
                                font-family="Bebas Neue" font-weight="400" font-size="120" fill="#fff">"60 FPS"</text>
                            <text x={c.x + 40.0} y={baseline(c.y + 180.0, 40.0, 30.0, DM_SANS.0, DM_SANS.1)}
                                font-family="DM Sans" font-weight="500" font-size="30" fill="#fff">"rendered on the GPU"</text>
                        </g>
                    ),
                    _ => fframes::svgr!(
                        <g>
                            <rect x={c.x + 0.75} y={c.y + 0.75} width={c.w - 1.5} height={c.h - 1.5} rx="31.25"
                                fill="#fff" fill-opacity="0.12" stroke="#fff" stroke-opacity="0.35" stroke-width="1.5" />
                            <text x={c.x + 41.5} y={baseline(c.y + 71.5, 50.0, 40.0, DM_SANS.0, DM_SANS.1)}
                                font-family="DM Sans" font-weight="500" font-size="40" fill="#fff">"Layered cards"</text>
                            <text x={c.x + 41.5} y={baseline(c.y + 133.5, 36.0, 26.0, DM_SANS.0, DM_SANS.1)}
                                font-family="DM Sans" font-weight="500" font-size="26" fill="#fff" fill-opacity="0.7">
                                "shadows, glows, gradients"
                            </text>
                        </g>
                    ),
                };
                let glow_opacity = (0.55 + 0.25 * (2.0 * t + fi).sin()) * clamp01(p);
                fframes::svgr!(
                    <g transform={format!("translate(0 {dy})")}>
                        <rect x={c.x - 20.0} y={c.y - 20.0} width={c.w + 40.0} height={c.h + 40.0} rx="48"
                            fill={c.glow} opacity={glow_opacity} filter={format!("url(#{})", GLOW_IDS[i])} />
                        <g opacity={clamp01(p)}>
                            <rect x={c.x} y={c.y} width={c.w} height={c.h} rx="32" fill="#000"
                                filter={format!("url(#{})", SHADOW_IDS[i])} />
                            {content}
                        </g>
                    </g>
                )
            })
            .collect();

        // kinetic headline: one <text> per letter, laid out like inline-block spans
        let font = FontQuery { family: "Bebas Neue", size: 180, weight: 400, ..Default::default() };
        let mut j = 0usize;
        let mut letters: Vec<Svgr> = Vec::new();
        for (top, words) in HEADLINE {
            let mut x = 160.0;
            let base = baseline(top, 180.0, 180.0, BEBAS.0, BEBAS.1);
            for word in words {
                for ch in word.chars() {
                    let at = (ch as u8 - b'A') as usize;
                    let letter = &LETTERS[at..at + 1];
                    let w = frame.text_width(ctx, font, letter).unwrap_or(0) as f64;
                    let p = spring(&frame, 0.15 + j as f64 * 0.05, &self.letter_spring);
                    let wave = 6.0 * (3.0 * t + 0.5 * j as f64).sin();
                    let tf = around(x + w / 2.0, top + 180.0, 0.0, (1.0 - p) * 100.0 + wave, &format!("rotate({})", (1.0 - p) * -25.0));
                    letters.push(fframes::svgr!(
                        <text x={x} y={base} opacity={clamp01(p * 1.5)} transform={tf}
                            font-family="Bebas Neue" font-weight="400" font-size="180" fill="#fff">{letter}</text>
                    ));
                    x += w;
                    j += 1;
                }
                x += 40.0;
            }
        }
        let sub = ease_out_cubic((t - 1.0) / 0.6);

        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={Self::WIDTH} height={Self::HEIGHT}>
                <defs>
                    <filter id="backdrop-blur" filterUnits="userSpaceOnUse" x="-90" y="-90" width="2100" height="1260">
                        <feGaussianBlur stdDeviation="30" />
                    </filter>
                    <filter id="headline-glow" filterUnits="userSpaceOnUse" x="60" y="20" width="1300" height="620">
                        <feDropShadow dx="0" dy="0" stdDeviation="15" flood-color="#a855f7" flood-opacity="0.8" />
                    </filter>
                    <linearGradient id="overlay" x1="0" y1="0" x2="1" y2="1">
                        <stop offset="0" stop-color="#0f0a28" stop-opacity="0.85" />
                        <stop offset="1" stop-color="#3c145a" stop-opacity="0.6" />
                    </linearGradient>
                    <linearGradient id="card-grad" x1="0" y1="0" x2="1" y2="1">
                        <stop offset="0" stop-color="#4f46e5" />
                        <stop offset="1" stop-color="#9333ea" />
                    </linearGradient>
                    {defs}
                </defs>
                <rect width={Self::WIDTH} height={Self::HEIGHT} fill="#0b0a16" />
                <image href={poster.href()} x="0" y="0" width="1920" height="1080" preserveAspectRatio="none"
                    filter="url(#backdrop-blur)" transform={around(960.0, 540.0, 0.0, 0.0, &format!("scale({})", 1.1 + 0.01 * t))} />
                <rect width="1920" height="1080" fill="url(#overlay)" />
                {shapes}
                {cards}
                <g filter="url(#headline-glow)">{letters}</g>
                <text x={164.0 + (1.0 - sub) * -40.0} y={baseline(480.0, 50.0, 36.0, DM_SANS.0, DM_SANS.1)} opacity={sub}
                    font-family="DM Sans" font-weight="500" font-size="36" fill="#fff" fill-opacity="0.85">
                    "fframes · Rust · Skia GPU"
                </text>
            </svg>
        )
    }
}

fn main() {
    let spring = |stiffness, damping| AnimationRuntime::new(3.0, &Easing::Spring { mass: 1.0, stiffness, damping });
    let video = Motion {
        frames: 600,
        star: star_path(),
        card_spring: spring(90.0, 14.0),
        letter_spring: spring(140.0, 12.0),
    };
    shared::run("motion", &video, video.frames);
}
