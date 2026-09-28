//! Workload "TextFx" (fframes side). Twin of ../remotion/src/TextFx.tsx: 3,334 text nodes
//! per frame (300 frames, 1,000,200 in total), 18 to 96 px, each with seeded "random"
//! animated effects: rotation / scale / skew, opacity, HSL colour cycling, gradient fills,
//! outlined text, drop shadows, glow (every 10th node), letter-spacing and per-letter
//! wave words. Gradients and filters are shared `<defs>`, animated per frame.
//!
//! ```sh
//! DYLD_LIBRARY_PATH=/opt/homebrew/lib target/release/textfx metal out/textfx.mp4 medium
//! ```
//! Backends: `metal`, `vulkan` (MoltenVK), `cpu`. `GPU_CONTEXTS=n` for n Skia contexts.

#[path = "../shared.rs"]
mod shared;

use std::path::Path;
use std::time::Instant;

use fframes::{
    AudioMap, Color, Duration, EncoderOptions, FFramesContext, Frame, MediaDirectory, RenderOptions,
    Svgr, Video, fframes_logger::FFramesLoggerVariant,
};
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig};
use shared::hsl_to_hex;

const NODES: usize = 3334;
const FRAMES: usize = 300;
const WORDS: [&str; 20] = [
    "fframes", "Rust", "Skia", "GPU", "render", "SVG", "frame", "video", "motion", "text",
    "1,000,000", "60 fps", "pixels", "shader", "glyph", "kerning", "encode", "H.264", "effects", "wave",
];
const WAVE_WORDS: [&str; 4] = ["fframes", "WAVE", "rendering", "motion"];
const GRADIENTS: usize = 6;
const SHADOW_COLORS: [&str; 4] = ["#ff2d95", "#00e5ff", "#ffd400", "#7c4dff"];

/// lowbias32 integer hash; `hash32` in TextFx.tsx is the same function.
fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    x
}

/// Seeded random number in [0, 1) for node `i`, stream `k`.
fn rnd(i: usize, k: u32) -> f64 {
    hash32((i as u32) * 16 + k + 12345) as f64 / 4294967296.0
}

#[derive(PartialEq, Debug)]
enum Kind {
    Solid,
    Gradient,
    Stroke,
    Shadow,
    Spacing,
    Glow,
    Wave,
}

/// `only`: debug aid for the visual check (`TEXTFX_ONLY=stroke` draws only the stroke nodes);
/// unset in every timed run.
struct TextFx {
    only: Option<String>,
}

impl Video for TextFx {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(FRAMES)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let f = frame.index as f64;
        let fi = frame.index;

        // Shared, per-frame animated defs.
        let mut defs: Vec<Svgr> = Vec::with_capacity(GRADIENTS + SHADOW_COLORS.len() + 1);
        for v in 0..GRADIENTS {
            let base = v as f64 * 60.0 + f * 4.0;
            let c0 = hsl_to_hex(base % 360.0, 0.9, 0.6);
            let c1 = hsl_to_hex((base + 120.0) % 360.0, 0.9, 0.6);
            let c2 = hsl_to_hex((base + 240.0) % 360.0, 0.9, 0.6);
            defs.push(fframes::svgr!(
                <linearGradient id={format!("grad{v}")} x1="0" y1="0" x2="1" y2="0">
                    <stop offset="0" stop-color={c0} />
                    <stop offset="0.5" stop-color={c1} />
                    <stop offset="1" stop-color={c2} />
                </linearGradient>
            ));
        }
        for (v, color) in SHADOW_COLORS.iter().enumerate() {
            let a = f * 0.08 + v as f64 * 1.5708;
            let (dx, dy) = (5.0 * a.cos(), 5.0 * a.sin());
            defs.push(fframes::svgr!(
                <filter id={format!("shadow{v}")} x="-0.5" y="-1" width="2" height="3">
                    <feDropShadow dx={format!("{dx:.3}")} dy={format!("{dy:.3}")} stdDeviation="3" flood-color={*color} />
                </filter>
            ));
        }
        let glow_sigma = 3.0 + 2.0 * (0.5 + 0.5 * (f * 0.1).sin());
        defs.push(fframes::svgr!(
            <filter id="glow" x="-0.5" y="-1" width="2" height="3">
                <feGaussianBlur in="SourceGraphic" stdDeviation={format!("{glow_sigma:.3}")} result="b" />
                <feMerge>
                    <feMergeNode in="b" />
                    <feMergeNode in="b" />
                    <feMergeNode in="SourceGraphic" />
                </feMerge>
            </filter>
        ));

        let mut nodes: Vec<Svgr> = Vec::with_capacity(NODES);
        for i in 0..NODES {
            let r = |k| rnd(i, k);
            let kind = if i % 10 == 0 {
                Kind::Glow
            } else if i % 97 == 5 {
                Kind::Wave
            } else {
                match (r(11) * 5.0) as usize {
                    0 => Kind::Solid,
                    1 => Kind::Gradient,
                    2 => Kind::Stroke,
                    3 => Kind::Shadow,
                    _ => Kind::Spacing,
                }
            };
            if let Some(only) = &self.only
                && !format!("{kind:?}").eq_ignore_ascii_case(only)
            {
                continue;
            }
            let mut size = (18.0 + 78.0 * r(0).powf(1.6)).round();
            if matches!(kind, Kind::Glow | Kind::Shadow) {
                size = size.max(36.0);
            }
            if kind == Kind::Wave {
                size = (56.0 + 40.0 * r(0)).round();
            }
            let phase = r(3) * std::f64::consts::TAU;
            let speed = 0.5 + r(4);
            let x = -60.0 + r(1) * 1900.0 + 40.0 * (f * 0.03 * speed + phase).sin();
            let y = 40.0 + r(2) * 1060.0 + 30.0 * (f * 0.025 * speed + phase * 1.3).cos();
            let rot = if r(6) < 0.15 {
                f * 3.0 * speed + r(5) * 360.0
            } else {
                (r(5) - 0.5) * 60.0 + 25.0 * (f * 0.04 * speed + phase).sin()
            };
            let scale = 0.8 + 0.35 * (f * 0.06 * speed + phase * 0.7).sin();
            let skew = if r(7) < 0.4 { 20.0 * (f * 0.05 + phase).sin() } else { 0.0 };
            let opacity = 0.35 + 0.65 * (0.5 + 0.5 * (f * 0.07 * speed + phase * 2.0).sin());
            let hue = (r(8) * 360.0 + f * 3.0 * speed) % 360.0;
            let color = hsl_to_hex(hue, 0.85, 0.62);
            let label = if kind == Kind::Wave {
                WAVE_WORDS[(r(10) * WAVE_WORDS.len() as f64) as usize].to_string()
            } else if r(9) < 0.2 {
                ((fi * 37 + i * 101) % 100000).to_string()
            } else {
                WORDS[(r(10) * WORDS.len() as f64) as usize].to_string()
            };
            let tf = format!("translate({x:.3} {y:.3}) rotate({rot:.3}) skewX({skew:.3}) scale({scale:.4})");
            let op = format!("{opacity:.3}");

            nodes.push(match kind {
                Kind::Solid => fframes::svgr!(
                    <text transform={tf} opacity={op} font-size={size} fill={color}>{label}</text>
                ),
                Kind::Gradient => fframes::svgr!(
                    <text transform={tf} opacity={op} font-size={size}
                        fill={format!("url(#grad{})", (r(12) * GRADIENTS as f64) as usize)}>{label}</text>
                ),
                Kind::Stroke => fframes::svgr!(
                    <text transform={tf} opacity={op} font-size={size} fill="none" stroke={color}
                        stroke-width={format!("{:.3}", (size / 28.0).max(1.5))}>{label}</text>
                ),
                Kind::Shadow => fframes::svgr!(
                    <text transform={tf} opacity={op} font-size={size} fill={color}
                        filter={format!("url(#shadow{})", (r(13) * 4.0) as usize)}>{label}</text>
                ),
                Kind::Spacing => {
                    let ls = size * 0.25 * (0.5 + 0.5 * (f * 0.1 + phase).sin());
                    fframes::svgr!(
                        <text transform={tf} opacity={op} font-size={size} fill={color}
                            letter-spacing={format!("{ls:.3}")}>{label}</text>
                    )
                }
                Kind::Glow => fframes::svgr!(
                    <text transform={tf} opacity={op} font-size={size} fill={color} filter="url(#glow)">{label}</text>
                ),
                Kind::Wave => {
                    // one absolute y per letter: the letters bob on a sine
                    let ys: Vec<String> = (0..label.chars().count())
                        .map(|j| format!("{:.3}", 0.25 * size * (f * 0.2 + j as f64 * 0.6).sin()))
                        .collect();
                    fframes::svgr!(
                        <text transform={tf} opacity={op} font-size={size} fill={color} font-kerning="none"
                            y={ys.join(" ")}>{label}</text>
                    )
                }
            });
        }

        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={Self::WIDTH} height={Self::HEIGHT}>
                <defs>{defs}</defs>
                <rect width={Self::WIDTH} height={Self::HEIGHT} fill="#0b1020" />
                <g font-family="DM Sans" font-weight="400">{nodes}</g>
            </svg>
        )
    }
}

fn pipeline() -> SkiaPipelineConfig {
    let contexts = std::env::var("GPU_CONTEXTS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    SkiaPipelineConfig { concurrency_policy: SkiaPipelineConcurrencyPolicy::Concurrency(contexts), ..Default::default() }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let backend = args.next().unwrap_or_else(|| "metal".into());
    let out = args.next().unwrap_or_else(|| "out/textfx.mp4".into());
    let preset = args.next().unwrap_or_else(|| "medium".into());
    if let Some(dir) = Path::new(&out).parent() {
        std::fs::create_dir_all(dir).ok();
    }

    let started = Instant::now();
    // Only DMSans-Regular.ttf (byte-identical to ../remotion/public/DMSans-Regular.ttf).
    let media_dir = MediaDirectory::read_folder(concat!(env!("CARGO_MANIFEST_DIR"), "/media")).unwrap();
    let media = media_dir.process_media_source().unwrap();
    let options = RenderOptions {
        media: Some(&media),
        load_system_fonts: false,
        default_font: "DM Sans",
        logger: FFramesLoggerVariant::Compact,
        video_encoder_options: EncoderOptions {
            preferred_encoder: Some("libx264"),
            codec_params: Some(&[("crf", "18"), ("preset", preset.as_str())]),
            ..Default::default()
        },
        ..Default::default()
    };
    let video = TextFx { only: std::env::var("TEXTFX_ONLY").ok() };
    eprintln!("fframes textfx: {NODES} text nodes/frame x {FRAMES} frames, backend={backend}, 1920x1080@30 (libx264 crf 18, preset {preset})");
    let result = match backend.as_str() {
        "cpu" => fframes::render(&out, &video, fframes::cpu::CpuRenderingBackend::default(), &options),
        "vulkan" => {
            let ctx = fframes_skia_renderer::vulkan::SkiaVulkanCtx::new(1920, 1080).expect("vulkan");
            fframes::render(&out, &video, SkiaFFramesRenderer::new_vulkan(&ctx, pipeline()).unwrap(), &options)
        }
        #[cfg(target_os = "macos")]
        "metal" => {
            let ctx = fframes_skia_renderer::metal::SkiaMetalCtx::new(1920, 1080).expect("metal");
            fframes::render(&out, &video, SkiaFFramesRenderer::new_metal(&ctx, pipeline()).unwrap(), &options)
        }
        other => panic!("unknown backend {other}"),
    };
    result.expect("render failed");
    let secs = started.elapsed().as_secs_f64();
    eprintln!("fframes textfx: {FRAMES} frames to {out} in {secs:.2}s ({:.1} fps)", FRAMES as f64 / secs);
}
