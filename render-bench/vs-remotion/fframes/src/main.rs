//! fframes side of the Remotion vs fframes benchmark: 300 frames x 3,334 <text> nodes
//! (1,000,200 text nodes), 1920x1080 @ 30 fps, H.264 (libx264, crf 18, preset medium,
//! yuv420p: the same encoder settings `remotion render --codec=h264` uses by default).
//!
//! ```sh
//! cargo build --release -p vs-remotion-bench
//! DYLD_LIBRARY_PATH=/opt/homebrew/lib target/release/vs-remotion-bench metal out.mp4
//! ```
//! Backends: `metal`, `vulkan` (MoltenVK on macOS), `cpu` (svgr / tiny-skia).

use std::path::Path;
use std::time::Instant;

use fframes::{
    AudioMap, Color, Duration, EncoderOptions, FFramesContext, Frame, MediaDirectory,
    RenderOptions, Svgr, Video, fframes_logger::FFramesLoggerVariant,
};
use fframes_skia_renderer::{
    SkiaFFramesRenderer, SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig,
};

/// Skia pipeline config. `GPU_CONTEXTS=n` renders on n GPU contexts in parallel (default 1).
fn pipeline() -> SkiaPipelineConfig {
    let contexts = std::env::var("GPU_CONTEXTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    SkiaPipelineConfig {
        concurrency_policy: SkiaPipelineConcurrencyPolicy::Concurrency(contexts),
        ..Default::default()
    }
}

const NODES: usize = 3334;
const COLS: usize = 47;
const ROWS: usize = 71;
const CELL_W: f64 = 1920.0 / COLS as f64;
const CELL_H: f64 = 1080.0 / ROWS as f64;

struct TextGrid;

/// Same function as `hslToRgb` in ../remotion/src/TextGrid.tsx.
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
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
    (q(r), q(g), q(b))
}

impl Video for TextGrid {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(300)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let f = frame.index as f64;
        let fi = frame.index;
        let mut nodes: Vec<Svgr> = Vec::with_capacity(NODES);
        for i in 0..NODES {
            let fl = i as f64;
            let col = (i % COLS) as f64;
            let row = (i / COLS) as f64;
            let x = col * CELL_W + 2.0 + 4.0 * (f * 0.12 + fl * 0.37).sin();
            let y = row * CELL_H + 11.0 + 3.0 * (f * 0.09 + fl * 0.23).cos();
            let alpha = 0.3 + 0.7 * (0.5 + 0.5 * (f * 0.2 + fl * 0.05).sin());
            let (r, g, b) = hsl_to_rgb((fl * 0.9 + f * 4.0) % 360.0, 0.75, 0.62);
            let label = if (i + fi).is_multiple_of(9) {
                "fframes".to_string()
            } else {
                ((fi * 37 + i * 101) % 10000).to_string()
            };
            nodes.push(fframes::svgr!(
                <text x={x} y={y} font-family="DM Sans" font-size="10"
                    fill={format!("#{r:02x}{g:02x}{b:02x}")} fill-opacity={format!("{alpha:.3}")}>
                    {label}
                </text>
            ));
        }

        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={Self::WIDTH} height={Self::HEIGHT}>
                <rect width={Self::WIDTH} height={Self::HEIGHT} fill="#0b1020" />
                {nodes}
            </svg>
        )
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let backend = args.next().unwrap_or_else(|| "metal".into());
    let out = args.next().unwrap_or_else(|| "out/fframes.mp4".into());
    // x264 preset; `medium` is the libx264 default that `remotion render` also uses.
    let preset = args.next().unwrap_or_else(|| "medium".into());
    if let Some(dir) = Path::new(&out).parent() {
        std::fs::create_dir_all(dir).ok();
    }

    let started = Instant::now();
    let media_dir =
        MediaDirectory::read_folder(concat!(env!("CARGO_MANIFEST_DIR"), "/media")).unwrap();
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

    let video = TextGrid;
    eprintln!(
        "fframes: {NODES} text nodes/frame x 300 frames, backend={backend}, 1920x1080@30 h264 (libx264 crf 18, preset {preset})"
    );
    let result = match backend.as_str() {
        "cpu" => fframes::render(
            &out,
            &video,
            fframes::cpu::CpuRenderingBackend::default(),
            &options,
        ),
        "vulkan" => {
            let ctx =
                fframes_skia_renderer::vulkan::SkiaVulkanCtx::new(1920, 1080).expect("vulkan");
            fframes::render(
                &out,
                &video,
                SkiaFFramesRenderer::new_vulkan(&ctx, pipeline()).unwrap(),
                &options,
            )
        }
        #[cfg(target_os = "macos")]
        "metal" => {
            let ctx = fframes_skia_renderer::metal::SkiaMetalCtx::new(1920, 1080).expect("metal");
            fframes::render(
                &out,
                &video,
                SkiaFFramesRenderer::new_metal(&ctx, pipeline()).unwrap(),
                &options,
            )
        }
        other => panic!("unknown backend {other}"),
    };
    result.expect("render failed");
    let secs = started.elapsed().as_secs_f64();
    eprintln!(
        "fframes: rendered 300 frames ({} text nodes) to {out} in {secs:.2}s ({:.1} fps)",
        NODES * 300,
        300.0 / secs
    );
}
