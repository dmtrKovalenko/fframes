//! Head-to-head CPU benchmark: the direct usvgr->Skia renderer vs svgr
//! (tiny-skia), fframes' two production rendering backends.
//!
//! Renders real animated 1080p frames from the low-poly-art owl example —
//! the repo's designated renderer stress test — through both engines, with
//! and without their respective caches, on a single thread.  Frame trees are
//! pre-generated once (tree building is identical for both engines and is
//! reported separately).
//!
//! Run with:
//! ```sh
//! cargo run --release -p fframes_skia_renderer --features vulkan --example svgr_vs_skia
//! ```

use std::time::{Duration, Instant};

use fframes::{
    CombinedMediaProvider, FFramesContext, FFramesMode, FFramesRendererRuntime, Frame,
    MediaProvider, TextCache, TimeBase, Video, VideoDecodersWorker, VideoSize, usvgr,
};
use fframes_skia_renderer::render::{RenderCache, render_tree};
use low_poly_art_example::{LowPolyMedia, LowPolyVideo, owl};

const FRAMES: usize = 300;
const WIDTH: i32 = 1920;
const HEIGHT: i32 = 1080;

struct PassResult {
    label: &'static str,
    total: Duration,
    frames: usize,
}

impl PassResult {
    fn report(&self) {
        let ms_per_frame = self.total.as_secs_f64() * 1000.0 / self.frames as f64;
        let fps = self.frames as f64 / self.total.as_secs_f64();
        println!(
            "{:<34} {:>8.2} ms/frame {:>8.1} fps  ({} frames in {:.2}s)",
            self.label,
            ms_per_frame,
            fps,
            self.frames,
            self.total.as_secs_f64()
        );
    }
}

fn render_skia(
    trees: &[usvgr::Tree],
    label: &'static str,
    mut cache_for_frame: impl FnMut(&mut RenderCache) -> &mut RenderCache,
) -> (PassResult, Vec<u8>) {
    let info = skia_safe::ImageInfo::new(
        (WIDTH, HEIGHT),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        skia_safe::ColorSpace::new_srgb(),
    );
    let mut surface = skia_safe::surfaces::raster(&info, None, None).expect("surface");
    // Production reads pixels back for the encoder every frame; include that.
    let mut readback = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    let mut cache = RenderCache::new();

    // Warmup (first frame pays one-time costs in both engines).
    surface.canvas().clear(skia_safe::Color::BLACK);
    render_tree(&trees[0], surface.canvas(), cache_for_frame(&mut cache));

    let start = Instant::now();
    for tree in trees {
        surface.canvas().clear(skia_safe::Color::BLACK);
        render_tree(tree, surface.canvas(), cache_for_frame(&mut cache));
        assert!(surface.read_pixels(&info, &mut readback, (WIDTH * 4) as usize, (0, 0)));
    }
    let total = start.elapsed();

    (
        PassResult {
            label,
            total,
            frames: trees.len(),
        },
        readback,
    )
}

fn render_svgr(trees: &[usvgr::Tree], label: &'static str, cached: bool) -> (PassResult, Vec<u8>) {
    let pixmap_pool = svgr::PixmapPool::new();
    let mut cache = if cached {
        svgr::SvgrCache::new(20)
    } else {
        svgr::SvgrCache::none()
    };
    let mut pixmap = svgr::tiny_skia::Pixmap::new(WIDTH as u32, HEIGHT as u32).expect("pixmap");
    let svgr_ctx = svgr::Context::new_from_pixmap_unsafe(&pixmap);

    // Warmup.
    pixmap.fill(svgr::tiny_skia::Color::BLACK);
    svgr::render(
        &trees[0],
        svgr::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
        &mut cache,
        &pixmap_pool,
        &svgr_ctx,
    );

    let start = Instant::now();
    for tree in trees {
        pixmap.fill(svgr::tiny_skia::Color::BLACK);
        svgr::render(
            tree,
            svgr::tiny_skia::Transform::default(),
            &mut pixmap.as_mut(),
            &mut cache,
            &pixmap_pool,
            &svgr_ctx,
        );
        // The encoder consumes pixmap.data() directly — no readback cost.
        std::hint::black_box(pixmap.data().as_ptr());
    }
    let total = start.elapsed();

    (
        PassResult {
            label,
            total,
            frames: trees.len(),
        },
        pixmap.data().to_vec(),
    )
}

/// Fraction of pixels with any channel differing by more than `tol`
/// (premultiplied RGBA over an identical opaque background).
fn diff_fraction(a: &[u8], b: &[u8], tol: u8) -> f64 {
    let bad = a
        .chunks(4)
        .zip(b.chunks(4))
        .filter(|(pa, pb)| (0..3).any(|c| pa[c].abs_diff(pb[c]) > tol))
        .count();
    bad as f64 / (a.len() / 4) as f64
}

fn main() {
    let media = LowPolyMedia::new().unwrap();
    let owl_media = owl::OwlMedia::new().unwrap();
    let video = LowPolyVideo {
        media: &media,
        scene: &owl::Owl { media: &owl_media },
    };
    let combined_media = CombinedMediaProvider::from([
        &media as &dyn MediaProvider,
        &owl_media as &dyn MediaProvider,
    ]);

    let scenes = video.define_scenes();
    let runtime = FFramesRendererRuntime::new(
        TimeBase {
            fps: LowPolyVideo::FPS,
            sample_rate: 44100,
        },
        &video,
        &scenes,
        Some(&combined_media),
    )
    .expect("runtime");

    let ctx = FFramesContext {
        time_base: runtime.time_base,
        current_video_size: VideoSize {
            width: LowPolyVideo::WIDTH,
            height: LowPolyVideo::HEIGHT,
        },
        abort_signal: None,
        duration_in_frames: runtime.timeline.duration_in_frames,
        mode: FFramesMode::Renderer,
        scenes: runtime.timeline.scenes.as_ref(),
        media_source: Some(&combined_media),
        font_source: Some(&runtime.font_source),
    };

    let frames = FRAMES.min(runtime.timeline.duration_in_frames);
    let usvg_options = usvgr::Options::default();
    let break_lines_cache = TextCache::new(10);
    let decoders = VideoDecodersWorker::new(1);
    let mut converter_cache = usvgr::Cache::new_with_text_cache(10);

    println!(
        "low-poly-art owl @ {}x{}, {} frames, single thread\n",
        WIDTH, HEIGHT, frames
    );

    let start = Instant::now();
    let trees: Vec<usvgr::Tree> = (0..frames)
        .map(|fr| {
            let frame = Frame::__internal_make_for_renderer(
                fr,
                fr,
                ctx.time_base.fps,
                break_lines_cache.clone(),
                decoders.clone(),
            );
            video
                .render_frame(frame, &ctx)
                .into_svg_tree(
                    &usvg_options,
                    &mut converter_cache,
                    runtime.font_source.as_db_ref(),
                )
                .expect("tree")
        })
        .collect();
    println!(
        "tree generation (shared by both engines): {:.2} ms/frame\n",
        start.elapsed().as_secs_f64() * 1000.0 / frames as f64
    );

    let (svgr_uncached, _) = render_svgr(&trees, "svgr (no cache)", false);
    svgr_uncached.report();
    let (svgr_cached, svgr_pixels) = render_svgr(&trees, "svgr + SvgrCache(20)", true);
    svgr_cached.report();

    let (skia_uncached, _) = render_skia(&trees, "skia direct (no cache)", |cache| {
        *cache = RenderCache::new();
        cache
    });
    skia_uncached.report();
    let (skia_cached, skia_pixels) =
        render_skia(&trees, "skia direct + RenderCache", |cache| cache);
    skia_cached.report();

    // Sanity: both engines drew the same last frame.
    let diff = diff_fraction(&skia_pixels, &svgr_pixels, 40);
    println!(
        "\nsanity: last frame differs on {:.2}% of pixels between engines",
        diff * 100.0
    );

    println!(
        "\nspeedups: svgr cache {:.2}x | skia cache {:.2}x | skia-vs-svgr (cached) {:.2}x | (uncached) {:.2}x",
        svgr_uncached.total.as_secs_f64() / svgr_cached.total.as_secs_f64(),
        skia_uncached.total.as_secs_f64() / skia_cached.total.as_secs_f64(),
        svgr_cached.total.as_secs_f64() / skia_cached.total.as_secs_f64(),
        svgr_uncached.total.as_secs_f64() / skia_uncached.total.as_secs_f64(),
    );
}
