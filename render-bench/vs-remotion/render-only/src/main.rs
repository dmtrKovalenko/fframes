//! Render-only twin of browser.jsx. No video encoder is instantiated.
use std::{hint::black_box, path::PathBuf, time::Instant};

use fframes::{AudioMap, Duration, FFramesContext, Frame, Previewer, RenderOptions, Svgr, Video};
use fframes_skia_renderer::{SkiaBackend, SkiaCpuCtx, SkiaFrameRenderer};

const SIDE: usize = 1000;

struct Grid {
    nodes: usize,
    frames: usize,
}

impl Video for Grid {
    const FPS: usize = 30;
    const WIDTH: usize = SIDE;
    const HEIGHT: usize = SIDE;
    fn duration(&self) -> Duration<'_> {
        Duration::Frames(self.frames)
    }
    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }
    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let nodes: Vec<_> = (0..self.nodes).map(|slot| {
            let id = (slot + frame.index * 37) % self.nodes;
            let r = (id * 13 + frame.index * 17) % 256;
            let g = (id * 7 + frame.index * 29) % 256;
            let b = (id * 3 + frame.index * 43) % 256;
            let fill = format!("#{r:02x}{g:02x}{b:02x}");
            fframes::svgr!(<rect x={slot % SIDE} y={slot / SIDE} width="1" height="1" fill={fill} />)
        }).collect();
        fframes::svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000">{nodes}</svg>)
    }
}

fn run(
    backend: &impl SkiaBackend,
    name: &str,
    nodes: usize,
    frames: usize,
    warmup: usize,
    out: &std::path::Path,
) {
    let video = Grid {
        nodes,
        frames: frames + warmup,
    };
    let options = RenderOptions {
        load_system_fonts: false,
        ..Default::default()
    };
    let mut preview = Previewer::new(&video, &options).expect("preview initialization");
    let mut renderer = SkiaFrameRenderer::new(backend);
    let mut samples = Vec::new();
    for frame in 0..frames + warmup {
        let start = Instant::now();
        // Includes scene generation, SVG conversion, rasterization, GPU sync/readback.
        let image = preview.render(frame, &mut renderer).expect("render frame");
        let render_ms = start.elapsed().as_secs_f64() * 1000.;
        // Chrome exposes screenshots as PNGs. Produce the same artifact in memory;
        // record image compression separately and include it in the comparable total.
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, SIDE as u32, SIDE as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Fast);
            encoder
                .write_header()
                .expect("PNG header")
                .write_image_data(&image.pixels)
                .expect("PNG pixels");
        }
        let total_ms = start.elapsed().as_secs_f64() * 1000.;
        black_box(&bytes);
        if frame >= warmup {
            samples.push(serde_json::json!({"frame": frame, "render_ms": render_ms, "png_ms": total_ms-render_ms, "total_ms": total_ms}));
            // Disk writes and correctness checks are excluded on both sides.
            std::fs::write(out.join(format!("{frame}.png")), &bytes)
                .expect("save verification PNG");
        }
    }
    println!(
        "{}",
        serde_json::json!({"backend": name, "nodes": nodes, "samples": samples})
    );
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        6,
        "usage: render-only-bench BACKEND NODES FRAMES WARMUP OUTPUT_DIR"
    );
    let nodes: usize = args[2].parse().expect("nodes");
    let frames: usize = args[3].parse().expect("frames");
    let warmup: usize = args[4].parse().expect("warmup");
    assert!((1..=SIDE * SIDE).contains(&nodes) && frames > 0 && warmup > 0);
    let out = PathBuf::from(&args[5]);
    std::fs::create_dir_all(&out).expect("output directory");
    match args[1].as_str() {
        "skia-cpu" => run(
            &SkiaCpuCtx::new(SIDE, SIDE),
            "skia-cpu",
            nodes,
            frames,
            warmup,
            &out,
        ),
        #[cfg(feature = "metal")]
        "metal" => run(
            &fframes_skia_renderer::metal::SkiaMetalCtx::new(SIDE, SIDE).expect("Metal context"),
            "metal",
            nodes,
            frames,
            warmup,
            &out,
        ),
        #[cfg(feature = "vulkan")]
        "vulkan" => run(
            &fframes_skia_renderer::vulkan::SkiaVulkanCtx::new(SIDE, SIDE).expect("Vulkan context"),
            "vulkan",
            nodes,
            frames,
            warmup,
            &out,
        ),
        _ => panic!("backend is not compiled in; use skia-cpu, or --features metal/vulkan"),
    }
}
