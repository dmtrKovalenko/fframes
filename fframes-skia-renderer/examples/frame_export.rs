use std::path::PathBuf;
use std::time::Instant;

use fframes::ffmpeg_sys_fframes::AVPixelFormat;
use fframes::{
    AudioMap, Color, CombinedMediaProvider, Duration, EncoderFrameRenderer, EncoderOptions,
    FFramesContext, FFramesMode, FFramesRenderBackend, FFramesRendererRuntime, Frame,
    MediaProvider, RenderOptions, Scenes, Svgr, TextCache, TimeBase, Video, VideoDecodersWorker,
    VideoEncoderInfo, VideoSize, usvgr,
};
use fframes_skia_renderer::{
    SkiaBackend, SkiaEncoderFrameRenderer, SkiaFFramesRenderer, SkiaFrameExport, SkiaPipelineConfig,
};
use low_poly_art_example::{LowPolyMedia, LowPolyVideo, owl};

const MODES: [SkiaFrameExport; 3] = [
    SkiaFrameExport::CpuConversion,
    SkiaFrameExport::GpuConversion,
    SkiaFrameExport::Auto,
];

/// Renders only the first `frames` frames of a video, without audio.
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

struct Args {
    frames: usize,
    encoder: Option<String>,
    pixel_format: AVPixelFormat,
    params: Vec<(String, String)>,
    out: PathBuf,
}

fn parse_args() -> Args {
    let mut args = Args {
        frames: 240,
        encoder: None,
        pixel_format: AVPixelFormat::AV_PIX_FMT_YUV420P,
        params: Vec::new(),
        out: std::env::temp_dir().join("fframes-frame-export"),
    };

    let mut input = std::env::args().skip(1);
    while let Some(flag) = input.next() {
        let mut value = || {
            input
                .next()
                .unwrap_or_else(|| panic!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--frames" => args.frames = value().parse().expect("--frames takes a number"),
            "--encoder" => args.encoder = Some(value()),
            "--out" => args.out = PathBuf::from(value()),
            "--param" => {
                let param = value();
                let (key, value) = param.split_once('=').expect("--param takes key=value");
                args.params.push((key.to_owned(), value.to_owned()));
            }
            "--pixel-format" => {
                args.pixel_format = match value().as_str() {
                    "yuv420p" => AVPixelFormat::AV_PIX_FMT_YUV420P,
                    "nv12" => AVPixelFormat::AV_PIX_FMT_NV12,
                    "yuva420p" => AVPixelFormat::AV_PIX_FMT_YUVA420P,
                    "yuv444p" => AVPixelFormat::AV_PIX_FMT_YUV444P,
                    other => panic!("unknown pixel format {other}"),
                };
            }
            other => panic!("unknown argument {other}"),
        }
    }
    args
}

#[cfg(feature = "vulkan-video")]
fn backend(width: usize, height: usize) -> impl SkiaBackend {
    use fframes_skia_renderer::vulkan::SkiaVulkanCtx;

    // Shares the device with FFmpeg when it can, which is what Vulkan Video encoders need.
    SkiaVulkanCtx::new_shared_with_encoder(width, height)
        .or_else(|_| SkiaVulkanCtx::new(width, height))
        .expect("a Vulkan device")
}

#[cfg(all(feature = "vulkan", not(feature = "vulkan-video")))]
fn backend(width: usize, height: usize) -> impl SkiaBackend {
    fframes_skia_renderer::vulkan::SkiaVulkanCtx::new(width, height).expect("a Vulkan device")
}

#[cfg(all(feature = "metal", not(feature = "vulkan")))]
fn backend(width: usize, height: usize) -> impl SkiaBackend {
    fframes_skia_renderer::metal::SkiaMetalCtx::new(width, height).expect("a Metal device")
}

#[cfg(not(any(feature = "metal", feature = "vulkan")))]
fn backend(width: usize, height: usize) -> impl SkiaBackend {
    eprintln!("no GPU feature enabled (vulkan or metal), rendering with Skia on the CPU");
    fframes_skia_renderer::SkiaCpuCtx::new(width, height)
}

fn main() {
    let args = parse_args();
    std::fs::create_dir_all(&args.out).expect("output directory");

    let media = LowPolyMedia::new().unwrap();
    let owl_media = owl::OwlMedia::new().unwrap();
    let full_video = LowPolyVideo {
        media: &media,
        scene: &owl::Owl { media: &owl_media },
    };
    let combined_media = CombinedMediaProvider::from([
        &media as &dyn MediaProvider,
        &owl_media as &dyn MediaProvider,
    ]);

    let scenes = full_video.define_scenes();
    let runtime = FFramesRendererRuntime::new(
        TimeBase {
            fps: LowPolyVideo::FPS,
            sample_rate: 44100,
        },
        &full_video,
        &scenes,
        Some(&combined_media),
    )
    .expect("runtime");
    let frames = args.frames.min(runtime.timeline.duration_in_frames);
    let (width, height) = (LowPolyVideo::WIDTH, LowPolyVideo::HEIGHT);

    let ctx = FFramesContext {
        time_base: runtime.time_base,
        current_video_size: VideoSize { width, height },
        abort_signal: None,
        duration_in_frames: runtime.timeline.duration_in_frames,
        mode: FFramesMode::Renderer,
        scenes: runtime.timeline.scenes.as_ref(),
        media_source: Some(&combined_media),
        font_source: Some(&runtime.font_source),
    };

    let usvg_options = usvgr::Options::default();
    let break_lines_cache = TextCache::new(10);
    let decoders = VideoDecodersWorker::new(1);
    let mut converter_cache = usvgr::Cache::new_with_text_cache(10);
    let trees: Vec<usvgr::Tree> = (0..frames)
        .map(|index| {
            let frame = Frame::__internal_make_for_renderer(
                index,
                index,
                ctx.time_base.fps,
                break_lines_cache.clone(),
                decoders.clone(),
            );
            full_video
                .render_frame(frame, &ctx)
                .into_svg_tree(
                    &usvg_options,
                    &mut converter_cache,
                    runtime.font_source.as_db_ref(),
                )
                .expect("tree")
        })
        .collect();

    let skia = backend(width, height);
    let params: Vec<(&str, &str)> = args
        .params
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let encoder_options = EncoderOptions {
        preferred_encoder: args.encoder.as_deref(),
        pixel_format: args.pixel_format,
        codec_params: (!params.is_empty()).then_some(params.as_slice()),
        ..Default::default()
    };
    let output = |mode: SkiaFrameExport| args.out.join(format!("{mode:?}.mp4"));

    let encoder = VideoEncoderInfo::for_output(
        &output(SkiaFrameExport::Auto),
        (width as i32, height as i32, LowPolyVideo::FPS as i32),
        &encoder_options,
    )
    .unwrap_or_else(|err| panic!("no encoder: {err}"));
    println!(
        "low-poly-art owl @ {width}x{height}, {frames} frames, encoder {}, {:?} requested\n",
        encoder.name(),
        args.pixel_format,
    );

    println!("stage: draw + export on one GPU context, no encoding");
    for mode in MODES {
        let backend =
            SkiaFFramesRenderer::new(SkiaPipelineConfig::default(), &skia).frame_export(mode);
        let input = match backend.negotiate_encoder_input(&encoder) {
            Ok(input) => input,
            Err(err) => {
                println!("  {:<14} skipped: {err}", format!("{mode:?}"));
                continue;
            }
        };
        let mut renderer =
            SkiaEncoderFrameRenderer::new(&skia, mode, &input, width as u32, height as u32)
                .expect("encoder frame renderer");

        // The first frame pays for shader compilation and cache warmup.
        drop(renderer.render_tree(&trees[0], LowPolyVideo::BACKGROUND_COLOR));

        let start = Instant::now();
        for tree in &trees {
            let frame = renderer
                .render_tree(tree, LowPolyVideo::BACKGROUND_COLOR)
                .expect("frame");
            std::hint::black_box(frame.as_ptr());
        }
        let elapsed = start.elapsed().as_secs_f64();
        println!(
            "  {:<14} {:>7.2} ms/frame {:>7.1} fps  {:?} as {:?}",
            format!("{mode:?}"),
            elapsed * 1000. / frames as f64,
            frames as f64 / elapsed,
            renderer.path(),
            input.pixel_format,
        );
    }

    println!("\ne2e: fframes::render with encoding");
    let video = Truncated {
        inner: &full_video,
        frames,
    };
    for mode in MODES {
        let renderer =
            SkiaFFramesRenderer::new(SkiaPipelineConfig::default(), &skia).frame_export(mode);
        if let Err(err) = renderer.negotiate_encoder_input(&encoder) {
            println!("  {:<14} skipped: {err}", format!("{mode:?}"));
            continue;
        }

        let chunks = args.out.join(format!("{mode:?}-chunks"));
        let start = Instant::now();
        let result = fframes::render(
            output(mode),
            &video,
            renderer,
            &RenderOptions {
                media: Some(&combined_media),
                logger: fframes::fframes_logger::FFramesLoggerVariant::Silent,
                tmp_files_directory: Some(&chunks),
                video_encoder_options: encoder_options.clone(),
                ..Default::default()
            },
        );
        let elapsed = start.elapsed().as_secs_f64();
        let _ = std::fs::remove_dir_all(&chunks);

        match result {
            Ok(()) => println!(
                "  {:<14} {:>7.1} fps  ({frames} frames in {elapsed:.2}s) -> {}",
                format!("{mode:?}"),
                frames as f64 / elapsed,
                output(mode).display(),
            ),
            Err(err) => println!("  {:<14} failed: {err:?}", format!("{mode:?}")),
        }
    }
}
