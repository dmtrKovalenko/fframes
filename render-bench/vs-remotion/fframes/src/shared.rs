//! Helpers shared by the realistic workloads (`src/bin/podcast.rs`, `src/bin/motion.rs`).
//! The math is a line-by-line twin of ../remotion/src/shared.ts, so both tools draw the
//! same frames.
//!
//! Command line of every workload binary:
//! `<bin> <metal|vulkan|cpu> <out.mp4> [x264 preset, default medium] [scale, default 1]`
//! plus `GPU_CONTEXTS=n` (Skia GPU contexts rendering in parallel, default 1).
#![allow(dead_code)]

use std::path::Path;
use std::time::Instant;

use fframes::{
    EncoderOptions, MediaDirectory, RenderOptions, Svgr, Video, fframes_logger::FFramesLoggerVariant,
};
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig};

pub fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

pub fn ease_out_cubic(v: f64) -> f64 {
    1.0 - (1.0 - clamp01(v)).powi(3)
}

pub fn hsl_to_hex(h: f64, s: f64, l: f64) -> String {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - ((hp % 2.0) - 1.0).abs());
    let (r, g, b) = if hp < 1.0 {
        (c, x, 0.0)
    } else if hp < 2.0 {
        (x, c, 0.0)
    } else if hp < 3.0 {
        (0.0, c, x)
    } else if hp < 4.0 {
        (0.0, x, c)
    } else if hp < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = l - c / 2.0;
    let q = |v: f64| ((v + m) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", q(r), q(g), q(b))
}

/// Baseline of a single text line that CSS centers in a `line-height` box starting at
/// `top` (half-leading model): the font's ascent/descent (hhea, in em) set the content box.
pub fn baseline(top: f64, line_height: f64, size: f64, ascent: f64, descent: f64) -> f64 {
    top + (line_height - (ascent + descent) * size) / 2.0 + ascent * size
}
pub const DM_SANS: (f64, f64) = (0.992, 0.31);
pub const BEBAS: (f64, f64) = (0.9, 0.3);

/// `translate(o + d) <op> translate(-o)`: a CSS transform with `transform-origin` at `o`.
pub fn around(ox: f64, oy: f64, dx: f64, dy: f64, op: &str) -> String {
    format!("translate({} {}) {op} translate({} {})", ox + dx, oy + dy, -ox, -oy)
}

pub const CAPTION_TEXT: &str = "so the thing nobody tells you about rendering video in code is that the browser \
was never built for it every frame goes through layout paint and a screenshot \
and then it has to be encoded which is fine for a demo but once you have real \
footage blurred backgrounds and captions on screen the costs add up fast and you \
start waiting on renders instead of shipping videos";
pub const WORD_SECONDS: f64 = 0.3;
pub const CAPTION_START: f64 = 0.4;
pub const WORDS_PER_LINE: usize = 6;

pub const SHAPES: usize = 40;
pub fn star_path() -> String {
    let pts: Vec<String> = (0..10)
        .map(|k| {
            let r = if k % 2 == 0 { 1.0 } else { 0.45 };
            let a = -std::f64::consts::PI / 2.0 + (k as f64 * std::f64::consts::PI) / 5.0;
            format!("{:.4} {:.4}", r * a.cos(), r * a.sin())
        })
        .collect();
    format!("M {} Z", pts.join(" L "))
}
pub const TRIANGLE_PATH: &str = "M 0 -1 L 0.866 0.5 L -0.866 0.5 Z";

/// CSS `box-shadow` (outer only, not painted under the element) for a rect: draw an opaque
/// rect with this filter. Region is in user space, so it moves with the element.
pub fn box_shadow_filter<'a>(id: &'a str, x: f64, y: f64, w: f64, h: f64, dy: f64, sigma: f64, opacity: f64) -> Svgr<'a> {
    let m = 3.0 * sigma;
    fframes::svgr!(
        <filter id={id} filterUnits="userSpaceOnUse" x={x - m} y={y - m} width={w + 2.0 * m} height={h + 2.0 * m + dy}>
            <feGaussianBlur in="SourceAlpha" stdDeviation={sigma} result="blur" />
            <feOffset in="blur" dx="0" dy={dy} result="off" />
            <feFlood flood-color="#000" flood-opacity={opacity} result="color" />
            <feComposite in="color" in2="off" operator="in" result="shadow" />
            <feComposite in="shadow" in2="SourceAlpha" operator="out" />
        </filter>
    )
}

/// CSS `filter: blur(sigma)` for something inside `x, y, w, h`.
pub fn blur_filter<'a>(id: &'a str, x: f64, y: f64, w: f64, h: f64, sigma: f64) -> Svgr<'a> {
    let m = 3.0 * sigma;
    fframes::svgr!(
        <filter id={id} filterUnits="userSpaceOnUse" x={x - m} y={y - m} width={w + 2.0 * m} height={h + 2.0 * m}>
            <feGaussianBlur stdDeviation={sigma} />
        </filter>
    )
}

fn pipeline() -> SkiaPipelineConfig {
    let contexts = std::env::var("GPU_CONTEXTS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    SkiaPipelineConfig {
        concurrency_policy: SkiaPipelineConcurrencyPolicy::Concurrency(contexts),
        ..Default::default()
    }
}

/// Renders `video` to the file named on the command line with libx264 crf 18 and prints the
/// wall time. Media (videos, poster.jpg, fonts) is read at runtime from ../remotion/public,
/// the same files Remotion uses.
pub fn run<V: Video + Send + Sync>(name: &str, video: &V, frames: usize) {
    let mut args = std::env::args().skip(1);
    let backend = args.next().unwrap_or_else(|| "metal".into());
    let out = args.next().unwrap_or_else(|| format!("out/{name}.mp4"));
    let preset = args.next().unwrap_or_else(|| "medium".into());
    let scale: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    if let Some(dir) = Path::new(&out).parent() {
        std::fs::create_dir_all(dir).ok();
    }

    let started = Instant::now();
    let media_dir =
        MediaDirectory::read_folder(concat!(env!("CARGO_MANIFEST_DIR"), "/../remotion/public")).unwrap();
    let media = media_dir.process_media_source().unwrap();
    let options = RenderOptions {
        media: Some(&media),
        load_system_fonts: false,
        default_font: "DM Sans",
        scale_resolution: scale,
        logger: FFramesLoggerVariant::Compact,
        video_encoder_options: EncoderOptions {
            preferred_encoder: Some("libx264"),
            codec_params: Some(&[("crf", "18"), ("preset", preset.as_str())]),
            ..Default::default()
        },
        ..Default::default()
    };
    let (w, h) = ((V::WIDTH as f64 * scale) as usize, (V::HEIGHT as f64 * scale) as usize);
    eprintln!("fframes {name}: {frames} frames, backend={backend}, {w}x{h}@{} (libx264 crf 18, preset {preset})", V::FPS);
    let result = match backend.as_str() {
        "cpu" => fframes::render(&out, video, fframes::cpu::CpuRenderingBackend::default(), &options),
        "vulkan" => {
            let ctx = fframes_skia_renderer::vulkan::SkiaVulkanCtx::new(w, h).expect("vulkan");
            fframes::render(&out, video, SkiaFFramesRenderer::new_vulkan(&ctx, pipeline()).unwrap(), &options)
        }
        #[cfg(target_os = "macos")]
        "metal" => {
            let ctx = fframes_skia_renderer::metal::SkiaMetalCtx::new(w, h).expect("metal");
            fframes::render(&out, video, SkiaFFramesRenderer::new_metal(&ctx, pipeline()).unwrap(), &options)
        }
        other => panic!("unknown backend {other}"),
    };
    result.expect("render failed");
    let secs = started.elapsed().as_secs_f64();
    eprintln!("fframes {name}: {frames} frames to {out} in {secs:.2}s ({:.1} fps)", frames as f64 / secs);
}
