//! Rendering benchmark over every example in the repository.
//!
//! Two kinds of measurements per example:
//!
//! * **stages** (single thread, the same frames for every engine): tree generation
//!   (`Video::render_frame` + `into_svg_tree`), CPU rasterization (svgr / tiny-skia),
//!   GPU rasterization (Skia over Vulkan, including flush and pixel readback) and the
//!   RGBA -> YUV420 conversion the encoder runs on every frame.
//! * **e2e**: the real `fframes::render` pipeline (all threads, encoding with
//!   libx264 ultrafast, no audio) over the first `--e2e-frames` frames, for the CPU
//!   backend and the Skia/Vulkan backend.
//!
//! ```sh
//! DYLD_LIBRARY_PATH=/opt/homebrew/lib cargo run --release -p render-bench -- --json before.json
//! DYLD_LIBRARY_PATH=/opt/homebrew/lib cargo run --release -p render-bench -- --compare before.json
//! ```
//!
//! `--dump DIR` stores the first frame of every window as PNG for both engines,
//! `--check DIR` compares the current output against such a dump.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use fframes::{
    AudioMap, Color, CombinedMediaProvider, Duration, EncoderOptions, FFramesContext, FFramesMode,
    FFramesRendererRuntime, Frame, MediaDirectory, MediaProvider, RenderOptions, Scenes,
    StaticMediaProvider, Svgr, TextCache, TimeBase, Video, VideoDecodersWorker, VideoSize,
    fframes_logger, usvgr,
};
use fframes_skia_renderer::vulkan::SkiaVulkanCtx;
use fframes_skia_renderer::{
    SkiaBackend, SkiaFFramesRenderer, SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig,
};
use serde::{Deserialize, Serialize};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

#[derive(Debug, Parser)]
struct Args {
    /// Comma separated example names, all examples when empty.
    #[clap(short, long, value_delimiter = ',')]
    examples: Vec<String>,
    /// Frames rendered per example in the stage benchmark.
    #[clap(long, default_value_t = 240)]
    frames: usize,
    /// Number of contiguous windows the stage frames are split into (spread over the video).
    #[clap(long, default_value_t = 4)]
    windows: usize,
    /// Repeat the stage windows this many times (handy for profiling).
    #[clap(long, default_value_t = 1)]
    repeat: usize,
    /// Skip the stage benchmark.
    #[clap(long)]
    no_stages: bool,
    /// Stage engines to run: cpu, gpu.
    #[clap(long, value_delimiter = ',', default_value = "cpu,gpu")]
    stage_engines: Vec<String>,
    /// End-to-end backends to run: cpu, gpu, gpu-max, or none.
    #[clap(long, value_delimiter = ',', default_value = "cpu,gpu")]
    e2e: Vec<String>,
    /// Frames rendered per example in the end-to-end benchmark.
    #[clap(long, default_value_t = 480)]
    e2e_frames: usize,
    /// Write the results as JSON.
    #[clap(long)]
    json: Option<PathBuf>,
    /// Print speedups against a previous `--json` result.
    #[clap(long)]
    compare: Option<PathBuf>,
    /// Save the first frame of every window as PNG (per engine).
    #[clap(long)]
    dump: Option<PathBuf>,
    /// Compare the first frame of every window with the PNG files saved by `--dump`.
    #[clap(long)]
    check: Option<PathBuf>,
    /// Directory for e2e video outputs.
    #[clap(long, default_value = "/tmp/fframes-render-bench")]
    out_dir: PathBuf,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Report {
    example: String,
    width: usize,
    height: usize,
    total_frames: usize,
    stage_frames: usize,
    /// ms per frame
    stages: BTreeMap<String, f64>,
    /// frames per second
    e2e: BTreeMap<String, f64>,
}

struct Spec {
    name: &'static str,
    cpu_cache: usize,
    system_fonts: bool,
    default_font: &'static str,
}

impl Spec {
    fn new(name: &'static str, cpu_cache: usize) -> Self {
        Self {
            name,
            cpu_cache,
            system_fonts: true,
            default_font: "Arial",
        }
    }
}

/// Wraps a video to render only its first `frames` frames without audio.
struct Truncated<'v, V> {
    inner: &'v V,
    frames: usize,
}

impl<V: Video> Video for Truncated<'_, V> {
    const FPS: usize = V::FPS;
    const WIDTH: usize = V::WIDTH;
    const HEIGHT: usize = V::HEIGHT;
    const BACKGROUND_COLOR: Color = V::BACKGROUND_COLOR;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(self.frames)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn define_scenes(&self) -> Scenes<'_> {
        self.inner.define_scenes()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        self.inner.render_frame(frame, ctx)
    }
}

struct Bench<'x> {
    args: &'x Args,
    reports: Vec<Report>,
    check_failures: Vec<String>,
}

fn frame_windows(total: usize, frames: usize, windows: usize) -> Vec<std::ops::Range<usize>> {
    let frames = frames.min(total);
    let windows = windows.clamp(1, frames.max(1));
    let len = frames / windows;
    (0..windows)
        .map(|i| {
            let start = if windows == 1 {
                0
            } else {
                i * (total - len) / (windows - 1)
            };
            start..start + len
        })
        .collect()
}

fn save_png(path: &Path, width: usize, height: usize, rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(rgba))
        .expect("write png");
}

fn load_png(path: &Path) -> Option<Vec<u8>> {
    let decoder = png::Decoder::new(std::fs::File::open(path).ok()?);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    Some(buf)
}

/// (max channel diff, percentage of pixels differing by more than 8)
fn diff(a: &[u8], b: &[u8]) -> (u8, f64) {
    let mut max = 0;
    let mut bad = 0usize;
    for (pa, pb) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
        let d = (0..4).map(|c| pa[c].abs_diff(pb[c])).max().unwrap();
        max = max.max(d);
        if d > 8 {
            bad += 1;
        }
    }
    (max, bad as f64 * 100.0 / (a.len() / 4) as f64)
}

impl Bench<'_> {
    fn wants(&self, name: &str) -> bool {
        self.args.examples.is_empty() || self.args.examples.iter().any(|e| e == name)
    }

    fn snapshot(&mut self, name: &str, frame: usize, engine: &str, w: usize, h: usize, px: &[u8]) {
        let file = format!("{name}_{frame:06}_{engine}.png");
        if let Some(dir) = &self.args.dump {
            std::fs::create_dir_all(dir).unwrap();
            save_png(&dir.join(&file), w, h, px);
        }
        if let Some(dir) = &self.args.check {
            match load_png(&dir.join(&file)) {
                Some(expected) => {
                    let (max, pct) = diff(&expected, px);
                    if pct > 0.1 {
                        let msg = format!("{file}: max diff {max}, {pct:.3}% pixels > 8");
                        println!("  CHECK FAIL {msg}");
                        self.check_failures.push(msg);
                    }
                }
                None => println!("  CHECK: missing {file}"),
            }
        }
    }

    fn run<'m, V: Video + Send + Sync>(
        &mut self,
        spec: Spec,
        video: &'m V,
        media: &'m dyn MediaProvider<'m>,
    ) {
        if !self.wants(spec.name) {
            return;
        }

        println!(
            "\n=== {} ({}x{} @ {}fps)",
            spec.name,
            V::WIDTH,
            V::HEIGHT,
            V::FPS
        );
        let mut report = Report {
            example: spec.name.to_owned(),
            width: V::WIDTH,
            height: V::HEIGHT,
            ..Default::default()
        };

        if !self.args.no_stages {
            self.stages(&spec, video, media, &mut report);
        } else {
            let scenes = video.define_scenes();
            let runtime = FFramesRendererRuntime::new(
                TimeBase {
                    fps: V::FPS,
                    sample_rate: 44100,
                },
                video,
                &scenes,
                Some(media),
            )
            .expect("runtime");
            report.total_frames = runtime.timeline.duration_in_frames;
        }

        for backend in &self.args.e2e {
            if backend == "none" {
                continue;
            }
            let frames = self.args.e2e_frames.min(report.total_frames.max(1));
            let truncated = Truncated {
                inner: video,
                frames,
            };
            std::fs::create_dir_all(&self.args.out_dir).unwrap();
            let output = self
                .args
                .out_dir
                .join(format!("{}-{backend}.mp4", spec.name));
            let options = RenderOptions {
                media: Some(media),
                load_system_fonts: spec.system_fonts,
                default_font: spec.default_font,
                logger: if std::env::var_os("BENCH_DEBUG").is_some() {
                    fframes_logger::FFramesLoggerVariant::Debug
                } else {
                    fframes_logger::FFramesLoggerVariant::Silent
                },
                video_encoder_options: EncoderOptions {
                    preferred_encoder: Some("libx264"),
                    codec_params: Some(&[("preset", "ultrafast"), ("crf", "23")]),
                    ..Default::default()
                },
                ..Default::default()
            };

            let start = Instant::now();
            let result = match backend.as_str() {
                "cpu" => fframes::render(
                    &output,
                    &truncated,
                    fframes::cpu::CpuRenderingBackend {
                        cache_capacity: spec.cpu_cache,
                        ..Default::default()
                    },
                    &options,
                ),
                "gpu" | "gpu-max" => {
                    let vk = SkiaVulkanCtx::new(V::WIDTH, V::HEIGHT).expect("vulkan");
                    fframes::render(
                        &output,
                        &truncated,
                        SkiaFFramesRenderer::new_vulkan(
                            &vk,
                            SkiaPipelineConfig {
                                buffer_queue_size: 20,
                                concurrency_policy: if backend == "gpu-max" {
                                    SkiaPipelineConcurrencyPolicy::MaxPerformance
                                } else {
                                    SkiaPipelineConcurrencyPolicy::OnePipeline
                                },
                                ..Default::default()
                            },
                        )
                        .unwrap(),
                        &options,
                    )
                }
                other => panic!("unknown e2e backend {other}"),
            };
            let elapsed = start.elapsed().as_secs_f64();
            match result {
                Ok(()) => {
                    let fps = frames as f64 / elapsed;
                    println!(
                        "  e2e {backend:<8} {frames} frames in {elapsed:>6.2}s  {fps:>7.1} fps"
                    );
                    report.e2e.insert(backend.clone(), fps);
                }
                Err(err) => println!("  e2e {backend:<8} FAILED: {err:?}"),
            }
        }

        self.reports.push(report);
    }

    fn stages<'m, V: Video + Send + Sync>(
        &mut self,
        spec: &Spec,
        video: &'m V,
        media: &'m dyn MediaProvider<'m>,
        report: &mut Report,
    ) {
        let scenes = video.define_scenes();
        let mut runtime = FFramesRendererRuntime::new(
            TimeBase {
                fps: V::FPS,
                sample_rate: 44100,
            },
            video,
            &scenes,
            Some(media),
        )
        .expect("runtime");
        if spec.system_fonts {
            runtime.font_source.load_system_fonts();
        }

        let mut image_source = HashMap::new();
        media.populate_image_source(&mut image_source);
        let usvg_options = usvgr::Options {
            image_data: Some(&image_source),
            font_family: spec.default_font.to_owned(),
            ..Default::default()
        };

        let ctx = FFramesContext {
            time_base: runtime.time_base,
            mode: FFramesMode::Renderer,
            media_source: Some(media),
            duration_in_frames: runtime.timeline.duration_in_frames,
            scenes: runtime.timeline.scenes.as_ref(),
            font_source: Some(&runtime.font_source),
            abort_signal: None,
            current_video_size: VideoSize::new_scaled(V::WIDTH, V::HEIGHT, 1.0),
        };
        report.total_frames = ctx.duration_in_frames;

        let (w, h) = (V::WIDTH, V::HEIGHT);
        let bg = V::BACKGROUND_COLOR;
        let run_cpu = self.args.stage_engines.iter().any(|e| e == "cpu");
        let run_gpu = self.args.stage_engines.iter().any(|e| e == "gpu");

        // CPU engine state (mirrors CpuRenderingBackend)
        let pixmap_pool = svgr::PixmapPool::new();
        let mut svgr_cache = svgr::SvgrCache::new(spec.cpu_cache);
        let mut pixmap = svgr::tiny_skia::Pixmap::new(w as u32, h as u32).unwrap();
        let svgr_ctx = svgr::Context::new_from_pixmap_unsafe(&pixmap);
        let cpu_bg = svgr::tiny_skia::Color::from_rgba8(bg.r, bg.g, bg.b, bg.a);

        // GPU engine state (mirrors the skia pipeline renderer thread)
        let vk = run_gpu.then(|| SkiaVulkanCtx::new(w, h).expect("vulkan"));
        let mut gpu = vk.as_ref().map(|vk| {
            let (surface, direct) = vk.create_skia_surface().expect("surface");
            (
                surface,
                direct,
                fframes_skia_renderer::render::RenderCache::new(),
            )
        });
        let gpu_bg = skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b);
        let mut readback = vec![0u8; w * h * 4];

        // YUV planes
        let mut y_plane = vec![0u8; w * h];
        let mut u_plane = vec![0u8; w * h / 4];
        let mut v_plane = vec![0u8; w * h / 4];

        let text_cache = TextCache::new(10);
        let decoders = VideoDecodersWorker::new(20);
        let mut converter_cache = usvgr::Cache::new_with_text_cache(10);

        let mut t_gen = 0.0;
        let mut t_cpu = 0.0;
        let mut t_gpu = 0.0;
        let mut t_yuv = 0.0;
        let mut t_gpu_draw = 0.0;
        let mut t_gpu_sync = 0.0;
        let mut timed = 0usize;

        let windows: Vec<_> = (0..self.args.repeat.max(1))
            .flat_map(|_| {
                frame_windows(ctx.duration_in_frames, self.args.frames, self.args.windows)
            })
            .collect();
        let mut warm = false;
        for window in windows {
            for fr in window.clone() {
                let first_in_window = fr == window.start;
                let t = Instant::now();
                let frame = Frame::__internal_make_for_renderer(
                    fr,
                    fr,
                    V::FPS,
                    text_cache.clone(),
                    decoders.clone(),
                );
                let tree = video
                    .render_frame(frame, &ctx)
                    .into_svg_tree(
                        &usvg_options,
                        &mut converter_cache,
                        runtime.font_source.as_db_ref(),
                    )
                    .expect("tree");
                let gen_t = t.elapsed().as_secs_f64();

                let mut cpu = 0.0;
                let mut yuv = 0.0;
                if run_cpu {
                    let t = Instant::now();
                    pixmap.fill(cpu_bg);
                    svgr::render(
                        &tree,
                        svgr::tiny_skia::Transform::default(),
                        &mut pixmap.as_mut(),
                        &mut svgr_cache,
                        &pixmap_pool,
                        &svgr_ctx,
                    );
                    cpu = t.elapsed().as_secs_f64();

                    let t = Instant::now();
                    unsafe {
                        fframes::pix_fmt::fill_yuv420_from_rgba_pixmap_accelerated(
                            w as i32,
                            h as i32,
                            w as i32,
                            (w / 2) as i32,
                            (w / 2) as i32,
                            pixmap.data(),
                            y_plane.as_mut_ptr(),
                            u_plane.as_mut_ptr(),
                            v_plane.as_mut_ptr(),
                        );
                    }
                    yuv = t.elapsed().as_secs_f64();
                    std::hint::black_box(&y_plane);
                }

                let mut gpu_t = 0.0;
                if let Some((surface, direct, cache)) = gpu.as_mut() {
                    let t = Instant::now();
                    surface.canvas().clear(gpu_bg);
                    fframes_skia_renderer::render::render_tree(&tree, surface.canvas(), cache);
                    t_gpu_draw += t.elapsed().as_secs_f64();
                    let t_sync = Instant::now();
                    if let Some(direct) = direct.as_mut() {
                        direct.flush_submit_and_sync_cpu();
                    }
                    t_gpu_sync += t_sync.elapsed().as_secs_f64();
                    let info = surface.image_info();
                    let pm =
                        skia_safe::Pixmap::new(&info, &mut readback, info.min_row_bytes()).unwrap();
                    let image = surface.image_snapshot();
                    assert!(image.read_pixels_to_pixmap_with_context(
                        direct.as_mut(),
                        &pm,
                        (0, 0),
                        skia_safe::image::CachingHint::Allow,
                    ));
                    gpu_t = t.elapsed().as_secs_f64();
                }
                drop(tree);

                if first_in_window {
                    if run_cpu {
                        self.snapshot(spec.name, fr, "cpu", w, h, pixmap.data());
                    }
                    if run_gpu {
                        self.snapshot(spec.name, fr, "gpu", w, h, &readback);
                    }
                }

                // the very first frame pays one-time costs (shader compilation, font loading)
                if !warm {
                    warm = true;
                    continue;
                }
                t_gen += gen_t;
                t_cpu += cpu;
                t_gpu += gpu_t;
                t_yuv += yuv;
                timed += 1;
            }
        }

        let ms = |t: f64| t * 1000.0 / timed.max(1) as f64;
        report.stage_frames = timed;
        report.stages.insert("gen".into(), ms(t_gen));
        if run_cpu {
            report.stages.insert("cpu".into(), ms(t_cpu));
            report.stages.insert("yuv".into(), ms(t_yuv));
        }
        if run_gpu {
            report.stages.insert("gpu".into(), ms(t_gpu));
            report.stages.insert("gpu_draw".into(), ms(t_gpu_draw));
            report.stages.insert("gpu_sync".into(), ms(t_gpu_sync));
            println!(
                "  gpu split: record {:.2} ms | flush+sync {:.2} ms | readback {:.2} ms",
                ms(t_gpu_draw),
                ms(t_gpu_sync),
                ms(t_gpu - t_gpu_draw - t_gpu_sync)
            );
        }
        println!(
            "  stages ({timed} frames of {}): gen {:.2} ms | cpu raster {:.2} ms | gpu raster+readback {:.2} ms | yuv {:.2} ms",
            ctx.duration_in_frames,
            ms(t_gen),
            ms(t_cpu),
            ms(t_gpu),
            ms(t_yuv)
        );
    }
}

fn print_summary(reports: &[Report], baseline: Option<&[Report]>) {
    let stage_keys = ["gen", "cpu", "gpu", "yuv"];
    let e2e_keys: Vec<String> = {
        let mut keys: Vec<String> = reports.iter().flat_map(|r| r.e2e.keys().cloned()).collect();
        keys.sort();
        keys.dedup();
        keys
    };

    println!("\n{:-<110}", "");
    print!("{:<28}", "example (ms/frame | fps)");
    for k in stage_keys {
        print!("{:>14}", k);
    }
    for k in &e2e_keys {
        print!("{:>16}", format!("e2e {k}"));
    }
    println!();

    for r in reports {
        let base = baseline.and_then(|b| b.iter().find(|b| b.example == r.example));
        print!("{:<28}", r.example);
        for k in stage_keys {
            let cell = match (r.stages.get(k), base.and_then(|b| b.stages.get(k))) {
                (Some(v), Some(b)) => format!("{v:.2} ({:.1}x)", b / v),
                (Some(v), None) => format!("{v:.2}"),
                _ => "-".into(),
            };
            print!("{cell:>14}");
        }
        for k in &e2e_keys {
            let cell = match (r.e2e.get(k), base.and_then(|b| b.e2e.get(k))) {
                (Some(v), Some(b)) => format!("{v:.0} ({:.1}x)", v / b),
                (Some(v), None) => format!("{v:.0}"),
                _ => "-".into(),
            };
            print!("{cell:>16}");
        }
        println!();
    }
}

fn main() {
    let args = Args::parse();
    let mut bench = Bench {
        args: &args,
        reports: vec![],
        check_failures: vec![],
    };
    let root = PathBuf::from(ROOT);

    {
        use hello_world_example::{HelloWorldMedia, HelloWorldVideo};
        let media = HelloWorldMedia::prepare().unwrap();
        let video = HelloWorldVideo {
            media: &media,
            slug: "Renderer!",
        };
        bench.run(Spec::new("hello-world", 5), &video, &media);
    }

    {
        use low_poly_art_example::{LowPolyMedia, LowPolyVideo, owl};
        if bench.wants("low-poly-art") {
            let media = LowPolyMedia::new().unwrap();
            let owl_media = owl::OwlMedia::new().unwrap();
            let owl = owl::Owl { media: &owl_media };
            let video = LowPolyVideo {
                media: &media,
                scene: &owl,
            };
            let combined = CombinedMediaProvider::from([
                &media as &dyn MediaProvider,
                &owl_media as &dyn MediaProvider,
            ]);
            let mut spec = Spec::new("low-poly-art", 5);
            spec.system_fonts = false;
            bench.run(spec, &video, &combined);
        }
    }

    {
        use marketing_example::{MarketingMedia, MarketingVideo};
        let media = MarketingMedia::prepare().unwrap();
        let video = MarketingVideo {
            audio_track: "marketing.mp3",
            media: &media,
        };
        bench.run(Spec::new("marketing", 30), &video, &media);
    }

    {
        use motion_graphics_example::{
            InstallSceneVideo, MotionGraphicsMedia, MotionGraphicsVideo, QuoteCardVideo,
        };
        let media = MotionGraphicsMedia::prepare().unwrap();
        bench.run(
            Spec::new("motion-graphics", 10),
            &MotionGraphicsVideo { media: &media },
            &media,
        );
        bench.run(
            Spec::new("motion-graphics-install", 10),
            &InstallSceneVideo::new(&media),
            &media,
        );
        bench.run(
            Spec::new("motion-graphics-quote", 10),
            &QuoteCardVideo::new(&media, "SPEED\n!=\nFAST"),
            &media,
        );
    }

    if bench.wants("podcast") {
        let folder = MediaDirectory::read_folder(root.join("examples/podcast/media")).unwrap();
        let media = folder.process_media_source().unwrap();
        let video = podcast_example::PodcastVideo {
            goose_audio: "final.mp3",
            duck_audio: "final.mp3",
            guest_audio: "final.mp3",
        };
        let mut spec = Spec::new("podcast", 20);
        spec.system_fonts = false;
        bench.run(spec, &video, &media);
    }

    if bench.wants("tiktok") {
        use tiktok_example::{GooseMedia, GooseVideo};
        let media = GooseMedia::prepare().unwrap();
        let mut spec = Spec::new("tiktok", 200);
        spec.system_fonts = false;
        bench.run(spec, &GooseVideo { media: &media }, &media);
    }

    if bench.wants("audio-announce") {
        use audio_announce_example::{AudioAnnounce, AudioAnnounceMedia};
        let media = AudioAnnounceMedia::prepare().unwrap();
        let folder =
            MediaDirectory::read_folder(root.join("examples/audio-announce/dynamic_media"))
                .unwrap();
        let dynamic = folder.process_media_source().unwrap();
        let combined = CombinedMediaProvider::from([
            &media as &dyn MediaProvider,
            &dynamic as &dyn MediaProvider,
        ]);
        let video = AudioAnnounce {
            media: &media,
            font: None,
        };
        bench.run(Spec::new("audio-announce", 20), &video, &combined);
    }

    if bench.wants("conference") {
        use conference_splash_screen::{
            ConferenceMedia, ConferenceVideo, SpeakerScene, SponsorScene,
        };
        let media = ConferenceMedia::prepare().unwrap();
        let folder = MediaDirectory::read_folder(
            root.join("examples/conference-splash-screen/dynamic_media"),
        )
        .unwrap();
        let dynamic = folder.process_media_source().unwrap();
        let combined = CombinedMediaProvider::from([
            &media as &dyn MediaProvider,
            &dynamic as &dyn MediaProvider,
        ]);
        let video = ConferenceVideo {
            media: &media,
            sponsor_scene: SponsorScene { media: &media },
            speaker_scene: SpeakerScene {
                avatar: Some("thibaut.jpg"),
                media: &media,
                speaker_name: "Thibaut Mattio",
                talk_title: "A Vision for OCaml in the AI Era",
                talk_description: "OCaml my Caml this talk is great. Sit back and enjoy!",
            },
        };
        bench.run(Spec::new("conference", 200), &video, &combined);
    }

    if bench.wants("beta") {
        use beta_example::{BetaExamples, BetaVideo};
        use hello_world_example::{HelloWorldMedia, HelloWorldVideo};
        use marketing_example::{MarketingMedia, MarketingVideo};
        use podcast_example::PodcastVideo;
        use tiktok_example::{GooseMedia, GooseVideo};

        let tiktok_media = GooseMedia::prepare().unwrap();
        let marketing_media = MarketingMedia::prepare().unwrap();
        let hello_media = HelloWorldMedia::prepare().unwrap();
        let folder = MediaDirectory::read_folder(root.join("examples/beta/media")).unwrap();
        let fs_media = folder.process_media_source().unwrap();
        let media = CombinedMediaProvider::from([
            &tiktok_media as &dyn MediaProvider,
            &marketing_media as &dyn MediaProvider,
            &hello_media as &dyn MediaProvider,
            &fs_media as &dyn MediaProvider,
        ]);
        let video = BetaVideo {
            iphone_scene: beta_example::IphoneScene {
                hours: 12,
                minutes: 4,
            },
            beta_examples: BetaExamples {
                marketing_video: Arc::new(MarketingVideo {
                    audio_track: "beta.mp3",
                    media: &marketing_media,
                }),
                hello_world_video: Arc::new(HelloWorldVideo {
                    media: &hello_media,
                    slug: "Hello, Beta!",
                }),
                podcast_video: Arc::new(PodcastVideo {
                    duck_audio: "beta.mp3",
                    goose_audio: "beta.mp3",
                    guest_audio: "beta.mp3",
                }),
                tiktok_video: Arc::new(GooseVideo {
                    media: &tiktok_media,
                }),
            },
        };
        let mut spec = Spec::new("beta", 200);
        spec.default_font = "Inter";
        bench.run(spec, &video, &media);
    }

    if bench.wants("pixel-memory") {
        use pixel_memory_example::{PixelMedia, PixelVideo, RandomPhotos};
        use rand::SeedableRng;
        let media = PixelMedia::prepare().unwrap();
        let photos =
            MediaDirectory::read_folder(root.join("examples/pixel-memory/photos")).unwrap();
        let photos_media = photos.process_media_source().unwrap();
        let rng = &mut rand::rngs::StdRng::seed_from_u64(48);
        let random_photos = RandomPhotos::new_from_media_provider(rng, &photos_media);
        let song = "The_Farewell.mp3";
        let video = PixelVideo::new_random_scenes(
            song,
            "They say dogs live shorter lives because they already know how to love unconditionally",
            rng,
            Some(&media),
            random_photos,
        );
        let combined = CombinedMediaProvider::from([
            &media as &dyn MediaProvider,
            &photos_media as &dyn MediaProvider,
        ]);
        bench.run(Spec::new("pixel-memory", 300), &video, &combined);
    }

    if bench.wants("teej-podcast") {
        use teej_podcast_example::{Chapter, TeejPodcast};
        let folder =
            MediaDirectory::read_folder(root.join("examples/teej-podcast/dynamic_media")).unwrap();
        let media = folder.process_media_source().unwrap();
        let chapters = [
            Chapter::new("Introduction to Lunch Bites Podcast", "00:00"),
            Chapter::new("Tech Sponsorships and Streaming Quality", "01:51"),
            Chapter::new("Flexing in LA: The Casa Bonita Experience", "02:33"),
            Chapter::new("Viral Moments: The Post-It Note Debate", "03:00"),
            Chapter::new("Zuckerberg's Rebranding and Tech Culture", "04:51"),
            Chapter::new("The Intersection of Geek Culture and Popularity", "14:55"),
            Chapter::new("Psychedelic Fantasy Baseball and AI", "16:48"),
            Chapter::new("The Rise of Meme Coins", "17:52"),
            Chapter::new("Trump Coin and the Crypto Circus", "19:13"),
            Chapter::new("Fart Coin: The AI Millionaire", "21:50"),
            Chapter::new("OpenAI and the Future of AI Models", "25:46"),
            Chapter::new("Influencers and the Coding Landscape", "30:10"),
        ];
        let video = TeejPodcast::new(&chapters);
        bench.run(Spec::new("teej-podcast", 20), &video, &media);
    }

    let baseline: Option<Vec<Report>> = args
        .compare
        .as_ref()
        .map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap());
    print_summary(&bench.reports, baseline.as_deref());

    if let Some(path) = &args.json {
        std::fs::write(path, serde_json::to_string_pretty(&bench.reports).unwrap()).unwrap();
    }
    if !bench.check_failures.is_empty() {
        println!("\n{} frame checks failed", bench.check_failures.len());
        std::process::exit(1);
    }
}
