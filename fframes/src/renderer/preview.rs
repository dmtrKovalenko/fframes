//! Rendering individual frames quickly and repeatedly: previews, contact sheets, SVG dumps
//! and frame diagnostics. Unlike `fframes::render_frame` a `Previewer` resolves the timeline,
//! loads fonts and images once and keeps its caches between frames.
use super::FFramesRendererRuntime;
use super::renderer_error::{FFramesRendererError, FFramesRendererResult};
use crate::diagnostics::{self, Diagnostic};
use crate::{
    AudioTimelineSamples, Color, FFramesContext, Frame, RenderOptions, ResolvedRenderingTimeline,
    TextCache, TimeBase, TimelineIndex, Video, VideoDecodersWorker, VideoSize, usvgr,
};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Straight (not premultiplied) RGBA8 pixels of one frame.
#[derive(Clone)]
pub struct RgbaFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl std::fmt::Debug for RgbaFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RgbaFrame({}x{})", self.width, self.height)
    }
}

impl RgbaFrame {
    /// Wraps premultiplied RGBA pixels (what tiny-skia and Skia produce).
    pub fn from_premultiplied(width: u32, height: u32, mut pixels: Vec<u8>) -> Self {
        for px in pixels.as_chunks_mut::<4>().0 {
            let a = px[3];
            if a != 255 && a != 0 {
                for c in &mut px[..3] {
                    *c = ((u32::from(*c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
                }
            }
        }

        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn into_image(self) -> image::RgbaImage {
        image::RgbaImage::from_raw(self.width, self.height, self.pixels)
            .expect("RgbaFrame pixel buffer matches its size")
    }

    /// The pixels of a rectangle of the frame, clamped to the frame.
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> RgbaFrame {
        let x = x.min(self.width.saturating_sub(1));
        let y = y.min(self.height.saturating_sub(1));
        let width = width.clamp(1, self.width - x);
        let height = height.clamp(1, self.height - y);
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in y..y + height {
            let start = ((row * self.width + x) * 4) as usize;
            pixels.extend_from_slice(&self.pixels[start..start + (width * 4) as usize]);
        }
        RgbaFrame {
            width,
            height,
            pixels,
        }
    }

    pub fn save_png(&self, path: impl AsRef<Path>) -> FFramesRendererResult<()> {
        let path = path.as_ref();
        image::save_buffer(
            path,
            &self.pixels,
            self.width,
            self.height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|err| FFramesRendererError::ImageError((path.display().to_string(), err)))
    }
}

/// Rasterizes converted frames. Implementations keep their surfaces and caches between
/// calls, so rendering many frames through one renderer is cheap.
pub trait FrameRenderer {
    /// Whether this renderer draws [`usvgr::Node::FastShape`] natively, see
    /// [`super::FFramesRenderBackend::fast_shapes`].
    fn fast_shapes(&self) -> bool {
        false
    }

    /// SVG text cache capacity requested by this renderer. `None` keeps the
    /// previewer's current setting; zero disables SVG text layout caching.
    fn svg_text_cache_capacity(&self) -> Option<usize> {
        None
    }

    /// Renders `tree` scaled to `width`x`height` over `background`.
    fn render_tree(
        &mut self,
        tree: &usvgr::Tree,
        background: Color,
        width: u32,
        height: u32,
    ) -> FFramesRendererResult<RgbaFrame>;
}

// The default backend factory returns `None` even without the CPU renderer feature.
impl FrameRenderer for std::convert::Infallible {
    fn render_tree(
        &mut self,
        _tree: &usvgr::Tree,
        _background: Color,
        _width: u32,
        _height: u32,
    ) -> FFramesRendererResult<RgbaFrame> {
        match *self {}
    }
}

/// The transform that fits a tree into the output size. Videos that hardcode their `WIDTH`
/// as the `<svg width>` are scaled this way when rendering with `scale_resolution`.
pub fn fit_transform(tree: &usvgr::Tree, width: u32, height: u32) -> usvgr::Transform {
    let size = tree.size();
    let sx = width as f32 / size.width();
    let sy = height as f32 / size.height();
    if (sx - 1.).abs() < f32::EPSILON && (sy - 1.).abs() < f32::EPSILON {
        usvgr::Transform::default()
    } else {
        usvgr::Transform::from_scale(sx, sy)
    }
}

/// The built-in tiny-skia rasterizer as a `FrameRenderer`.
#[cfg(feature = "cpu_renderer")]
pub struct CpuFrameRenderer {
    pixmap: Option<svgr::tiny_skia::Pixmap>,
    cache: svgr::SvgrCache,
    pixmap_pool: svgr::PixmapPool,
}

#[cfg(feature = "cpu_renderer")]
impl CpuFrameRenderer {
    /// `cache_capacity` is the number of static subtrees kept rasterized between frames.
    pub fn new(cache_capacity: usize) -> Self {
        Self {
            pixmap: None,
            cache: svgr::SvgrCache::new(cache_capacity),
            pixmap_pool: svgr::PixmapPool::new(),
        }
    }
}

#[cfg(feature = "cpu_renderer")]
impl Default for CpuFrameRenderer {
    fn default() -> Self {
        Self::new(20)
    }
}

#[cfg(feature = "cpu_renderer")]
impl FrameRenderer for CpuFrameRenderer {
    fn render_tree(
        &mut self,
        tree: &usvgr::Tree,
        background: Color,
        width: u32,
        height: u32,
    ) -> FFramesRendererResult<RgbaFrame> {
        use svgr::tiny_skia;

        let pixmap = match self.pixmap.take() {
            Some(pixmap) if pixmap.width() == width && pixmap.height() == height => pixmap,
            _ => tiny_skia::Pixmap::new(width, height).ok_or_else(|| {
                FFramesRendererError::Internal(format!(
                    "can not allocate a {width}x{height} pixmap"
                ))
            })?,
        };
        let mut pixmap = pixmap;

        pixmap.fill(tiny_skia::Color::from_rgba8(
            background.r,
            background.g,
            background.b,
            background.a,
        ));

        let ctx = svgr::Context::new_from_pixmap_unsafe(&pixmap);
        svgr::render(
            tree,
            fit_transform(tree, width, height),
            &mut pixmap.as_mut(),
            &mut self.cache,
            &self.pixmap_pool,
            &ctx,
        );

        let frame = RgbaFrame::from_premultiplied(width, height, pixmap.data().to_vec());
        self.pixmap = Some(pixmap);
        Ok(frame)
    }
}

/// What the CLI's `preview` command asks a real-time player to do.
#[derive(Debug, Clone)]
pub struct PreviewRequest {
    pub start_frame: usize,
    pub autoplay: bool,
    pub looping: bool,
    pub audio: bool,
    /// `auto`, `metal`, `vulkan` or `cpu`.
    pub backend: String,
}

/// The resolved structure of a video: size, duration, scenes and audio tracks.
#[derive(Debug, Clone, Serialize)]
pub struct TimelineReport {
    pub fps: usize,
    pub width: usize,
    pub height: usize,
    pub duration_frames: usize,
    pub duration_seconds: f32,
    pub scenes: Vec<SceneReport>,
    pub audio: Vec<AudioTrackReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SceneReport {
    pub index: usize,
    pub name: String,
    pub full_name: String,
    /// Frames including overlaps with the neighbours, end exclusive.
    pub start_frame: usize,
    pub end_frame: usize,
    pub start_seconds: f32,
    pub end_seconds: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioTrackReport {
    pub file: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub mix: crate::TrackMix,
}

/// What the diagnostics found in one frame.
#[derive(Debug, Clone, Serialize)]
pub struct FrameReport {
    pub frame: usize,
    pub seconds: f32,
    pub scenes: Vec<String>,
    pub diagnostics: Vec<ReportedDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportedDiagnostic {
    pub severity: diagnostics::Severity,
    /// Same key in different frames means the same problem (see `Diagnostic::group_key`).
    pub key: String,
    pub message: String,
    #[serde(flatten)]
    pub diagnostic: Diagnostic,
}

impl From<Diagnostic> for ReportedDiagnostic {
    fn from(diagnostic: Diagnostic) -> Self {
        Self {
            severity: diagnostic.severity(),
            key: diagnostic.group_key(),
            message: diagnostic.to_string(),
            diagnostic,
        }
    }
}

/// A long lived session for rendering arbitrary frames of a video.
///
/// ```ignore
/// let mut previewer = fframes::Previewer::new(&video, &options)?;
/// let mut renderer = fframes::CpuFrameRenderer::default();
/// let frame = previewer.timeline().resolve_frame("Intro@1.5s")?;
/// previewer.render(frame, &mut renderer)?.save_png("intro.png")?;
/// ```
pub struct Previewer<'a, 'media, TVideo: Video> {
    video: &'a TVideo,
    runtime: FFramesRendererRuntime<'a>,
    options: RenderOptions<'a, 'media>,
    image_source: HashMap<String, Arc<usvgr::PreloadedImageData>>,
    size: VideoSize,
    timeline_index: TimelineIndex,
    worker: Worker,
    converter_text_cache_capacity: usize,
    svgr_images: crate::SvgrImages<'a>,
}

impl<'a, 'media: 'a, TVideo: Video> Previewer<'a, 'media, TVideo> {
    pub fn new(
        video: &'a TVideo,
        options: &RenderOptions<'a, 'media>,
    ) -> FFramesRendererResult<Self> {
        let scenes = video.define_scenes();
        let mut runtime = FFramesRendererRuntime::new(
            TimeBase {
                fps: TVideo::FPS,
                sample_rate: options.audio_encoder_options.sample_rate,
            },
            video,
            &scenes,
            options.media,
        )?;

        if options.load_system_fonts {
            runtime.font_source.fontdb.load_system_fonts();
        }

        let mut image_source = HashMap::new();
        if let Some(media) = options.media {
            media.populate_image_source(&mut image_source);
        }

        let timeline_index = TimelineIndex::new(
            TVideo::FPS,
            runtime.timeline.duration_in_frames,
            runtime.timeline.scenes.as_ref(),
        );

        Ok(Self {
            video,
            size: VideoSize::new_scaled(TVideo::WIDTH, TVideo::HEIGHT, options.scale_resolution),
            options: options.clone(),
            runtime,
            image_source,
            timeline_index,
            worker: Worker::new(10),
            converter_text_cache_capacity: 10,
            svgr_images: crate::SvgrImages::new(default_svgr_rasterizer()),
        })
    }

    /// Renders the trees of [`FFramesContext::render_svgr`] with `rasterizer`, usually
    /// what [`super::FFramesRenderBackend::svgr_rasterizer`] of the backend returns. The
    /// default is the CPU rasterizer.
    pub fn with_svgr_rasterizer(
        mut self,
        rasterizer: Option<Box<dyn crate::SvgrRasterizer + 'a>>,
    ) -> Self {
        self.svgr_images = crate::SvgrImages::new(rasterizer);
        self
    }

    /// Scenes and duration for resolving time specs like `Intro@1.5s`.
    pub fn timeline(&self) -> &TimelineIndex {
        &self.timeline_index
    }

    pub fn resolved_timeline(&self) -> &ResolvedRenderingTimeline<'a, AudioTimelineSamples> {
        &self.runtime.timeline
    }

    pub fn font_db(&self) -> &usvgr::fontdb::Database {
        self.runtime.font_source.as_db_ref()
    }

    /// Scenes, duration and audio tracks as resolved for rendering.
    pub fn timeline_report(&self) -> TimelineReport {
        let index = &self.timeline_index;
        let time_base = self.runtime.time_base;
        TimelineReport {
            fps: TVideo::FPS,
            width: TVideo::WIDTH,
            height: TVideo::HEIGHT,
            duration_frames: index.duration_in_frames,
            duration_seconds: index.duration_in_seconds(),
            scenes: index
                .scenes
                .iter()
                .map(|scene| SceneReport {
                    index: scene.index,
                    name: scene.name.clone(),
                    full_name: scene.full_name.clone(),
                    start_frame: scene.frames.start,
                    end_frame: scene.frames.end,
                    start_seconds: index.frame_to_seconds(scene.frames.start),
                    end_seconds: index.frame_to_seconds(scene.frames.end),
                })
                .collect(),
            audio: self
                .runtime
                .timeline
                .audio_map
                .as_ref()
                .map(|map| {
                    use crate::AudioTimelineUnit;
                    map.tracks()
                        .iter()
                        .map(|track| AudioTrackReport {
                            file: track.file.clone(),
                            start_seconds: track.range.start.to_seconds(&time_base),
                            end_seconds: track.range.end.to_seconds(&time_base),
                            mix: track.mix.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub fn media(&self) -> Option<&'media dyn crate::MediaProvider<'media>> {
        self.options.media
    }

    pub fn options(&self) -> &RenderOptions<'a, 'media> {
        &self.options
    }

    /// Output size in pixels (`WIDTH`x`HEIGHT` times `scale_resolution`).
    pub fn size(&self) -> (u32, u32) {
        (self.size.width as u32, self.size.height as u32)
    }

    /// Changes the output size of the following frames.
    pub fn set_scale(&mut self, scale: f64) {
        self.size = VideoSize::new_scaled(TVideo::WIDTH, TVideo::HEIGHT, scale);
    }

    /// Calls `Video::render_frame` and converts the result.
    pub fn svg_tree(&mut self, frame: usize) -> FFramesRendererResult<usvgr::Tree> {
        self.convert_frame(frame, false)
    }

    /// [`Self::svg_tree`] for a renderer, which may draw [`usvgr::Node::FastShape`] natively.
    fn convert_frame(
        &mut self,
        frame: usize,
        fast_shapes: bool,
    ) -> FFramesRendererResult<usvgr::Tree> {
        Conversion::new(
            self.video,
            &self.runtime,
            &self.options,
            &self.image_source,
            &self.size,
            &self.svgr_images,
            fast_shapes,
        )
        .convert(&mut self.worker, frame)
    }

    /// Converts `frames` and calls `each` with what `keep` keeps of every tree and the
    /// problems found in it, in the order of `frames`. Stops at the first error of `each`.
    ///
    /// With more than one thread, the threads call `Video::render_frame` and convert the
    /// frames while `each` works on this thread, see [`Schedule`].
    fn convert_each<T: Send>(
        &mut self,
        frames: &[usize],
        batch: Batch,
        keep: impl Fn(usvgr::Tree) -> T + Sync,
        mut each: impl FnMut(
            usize,
            FFramesRendererResult<T>,
            Vec<Diagnostic>,
        ) -> FFramesRendererResult<()>,
    ) -> FFramesRendererResult<()> {
        let conversion = Conversion::new(
            self.video,
            &self.runtime,
            &self.options,
            &self.image_source,
            &self.size,
            &self.svgr_images,
            batch.fast_shapes,
        );
        let convert = |worker: &mut Worker, frame: usize| {
            if !batch.inspect {
                return (conversion.convert(worker, frame).map(&keep), Vec::new());
            }
            let (tree, mut found) = diagnostics::collect(|| conversion.convert(worker, frame));
            if let Ok(tree) = &tree {
                found.extend(diagnostics::inspect_tree(
                    tree,
                    tree.size().width(),
                    tree.size().height(),
                ));
            }
            (tree.map(&keep), found)
        };

        let threads = batch.threads.min(frames.len());
        if threads <= 1 {
            for &frame in frames {
                let (kept, found) = convert(&mut self.worker, frame);
                each(frame, kept, found)?;
            }
            return Ok(());
        }
        let (run, window) = match batch.schedule {
            Schedule::Ahead => (1, threads * AHEAD_PER_THREAD),
            Schedule::Parts => (frames.len().div_ceil(threads), frames.len()),
        };
        let capacity = self.converter_text_cache_capacity;
        in_order(
            frames.len(),
            threads,
            run,
            window,
            || Worker::new(capacity),
            |worker, index| convert(worker, frames[index]),
            |index, (kept, found)| each(frames[index], kept, found),
        )
    }

    /// The frame as an SVG document, as the renderer sees it after conversion (text is
    /// already laid out, `use` and styles are resolved).
    pub fn svg(&mut self, frame: usize) -> FFramesRendererResult<String> {
        Ok(self
            .svg_tree(frame)?
            .to_string(&usvgr::WriteOptions::default()))
    }

    /// Renders a frame to pixels, over the video's `BACKGROUND_COLOR` exactly like the encoder.
    pub fn render(
        &mut self,
        frame: usize,
        renderer: &mut dyn FrameRenderer,
    ) -> FFramesRendererResult<RgbaFrame> {
        self.configure_renderer_cache(renderer);
        let tree = self.convert_frame(frame, renderer.fast_shapes())?;
        let (width, height) = self.size();
        renderer.render_tree(&tree, TVideo::BACKGROUND_COLOR, width, height)
    }

    fn configure_renderer_cache(&mut self, renderer: &dyn FrameRenderer) {
        if let Some(capacity) = renderer.svg_text_cache_capacity()
            && capacity != self.converter_text_cache_capacity
        {
            self.worker.converter_cache = usvgr::Cache::new_with_text_cache(capacity);
            self.converter_text_cache_capacity = capacity;
        }
    }

    /// Renders a frame and reports the problems found while converting it.
    pub fn render_inspected(
        &mut self,
        frame: usize,
        renderer: &mut dyn FrameRenderer,
    ) -> FFramesRendererResult<(RgbaFrame, FrameReport)> {
        let mut rendered = None;
        self.render_inspected_on(&[frame], renderer, 1, |pixels, report| {
            rendered = Some((pixels, report));
            Ok(())
        })?;
        Ok(rendered.expect("the frame is rendered or an error is returned"))
    }

    /// [`Self::render`] for several frames: `each` gets the pixels in the order of `frames`.
    /// Stops at the first error.
    ///
    /// Several threads call `Video::render_frame` and convert the next frames while
    /// `renderer` draws on this thread: a quarter of [`super::get_thread_count`]
    /// (`FFRAMES_NUM_THREADS`), at most four. With fewer than eight, the frames convert on
    /// this thread.
    pub fn render_each(
        &mut self,
        frames: &[usize],
        renderer: &mut dyn FrameRenderer,
        mut each: impl FnMut(usize, RgbaFrame) -> FFramesRendererResult<()>,
    ) -> FFramesRendererResult<()> {
        self.configure_renderer_cache(renderer);
        let (width, height) = self.size();
        let batch = Batch {
            fast_shapes: renderer.fast_shapes(),
            inspect: false,
            threads: convert_threads(),
            schedule: Schedule::Ahead,
        };
        self.convert_each(
            frames,
            batch,
            |tree| tree,
            |frame, tree, _| {
                let pixels =
                    renderer.render_tree(&tree?, TVideo::BACKGROUND_COLOR, width, height)?;
                each(frame, pixels)
            },
        )
    }

    /// [`Self::render_inspected`] for several frames, on several threads like
    /// [`Self::render_each`]. `each` gets the frames in the order of `frames`.
    pub fn render_inspected_each(
        &mut self,
        frames: &[usize],
        renderer: &mut dyn FrameRenderer,
        each: impl FnMut(RgbaFrame, FrameReport) -> FFramesRendererResult<()>,
    ) -> FFramesRendererResult<()> {
        self.render_inspected_on(frames, renderer, convert_threads(), each)
    }

    fn render_inspected_on(
        &mut self,
        frames: &[usize],
        renderer: &mut dyn FrameRenderer,
        threads: usize,
        mut each: impl FnMut(RgbaFrame, FrameReport) -> FFramesRendererResult<()>,
    ) -> FFramesRendererResult<()> {
        self.configure_renderer_cache(renderer);
        let (width, height) = self.size();
        let batch = Batch {
            fast_shapes: renderer.fast_shapes(),
            inspect: true,
            threads,
            schedule: Schedule::Ahead,
        };
        let index = self.timeline_index.clone();
        self.convert_each(
            frames,
            batch,
            |tree| tree,
            |frame, tree, found| {
                let pixels =
                    renderer.render_tree(&tree?, TVideo::BACKGROUND_COLOR, width, height)?;
                each(pixels, report(&index, frame, found))
            },
        )
    }

    /// Converts a frame and reports problems in it (missing media or fonts, clipped text,
    /// panics) without rasterizing it.
    pub fn inspect(&mut self, frame: usize) -> FFramesRendererResult<FrameReport> {
        let mut inspected = None;
        self.inspect_on(&[frame], 1, |report| {
            inspected = Some(report);
            Ok(())
        })?;
        Ok(inspected.expect("the frame is inspected or an error is returned"))
    }

    /// [`Self::inspect`] for several frames, on several threads like [`Self::render_each`].
    /// `each` gets the reports in the order of `frames`.
    pub fn inspect_each(
        &mut self,
        frames: &[usize],
        each: impl FnMut(FrameReport) -> FFramesRendererResult<()>,
    ) -> FFramesRendererResult<()> {
        self.inspect_on(frames, convert_threads(), each)
    }

    fn inspect_on(
        &mut self,
        frames: &[usize],
        threads: usize,
        mut each: impl FnMut(FrameReport) -> FFramesRendererResult<()>,
    ) -> FFramesRendererResult<()> {
        let batch = Batch {
            fast_shapes: false,
            inspect: true,
            threads,
            schedule: Schedule::Parts,
        };
        let index = self.timeline_index.clone();
        // Only the problems are needed, so the trees are dropped on the threads.
        self.convert_each(frames, batch, drop, |frame, tree, found| {
            match tree {
                // The panic is already reported as a diagnostic.
                Ok(()) | Err(FFramesRendererError::FramePanicked(_)) => {}
                Err(err) => return Err(err),
            }
            each(report(&index, frame, found))
        })
    }
}

fn report(index: &TimelineIndex, frame: usize, found: Vec<Diagnostic>) -> FrameReport {
    FrameReport {
        frame,
        seconds: index.frame_to_seconds(frame),
        scenes: index.scenes_at(frame).map(|s| s.name.clone()).collect(),
        diagnostics: found.into_iter().map(ReportedDiagnostic::from).collect(),
    }
}

/// The caches of a thread that converts frames.
struct Worker {
    text_cache: Option<TextCache>,
    decoders: VideoDecodersWorker,
    converter_cache: usvgr::Cache,
}

impl Worker {
    fn new(converter_text_cache_capacity: usize) -> Self {
        Self {
            text_cache: TextCache::new(10),
            decoders: VideoDecodersWorker::new(2),
            converter_cache: usvgr::Cache::new_with_text_cache(converter_text_cache_capacity),
        }
    }
}

/// What converting a frame needs besides the caches. The threads that convert frames share it.
struct Conversion<'p, 'media, TVideo> {
    video: &'p TVideo,
    ctx: FFramesContext<'p, 'media>,
    options: usvgr::Options<'p>,
    font_db: &'p usvgr::fontdb::Database,
    svgr_images: &'p crate::SvgrImages<'p>,
}

impl<'p, 'media: 'p, TVideo: Video> Conversion<'p, 'media, TVideo> {
    fn new(
        video: &'p TVideo,
        runtime: &'p FFramesRendererRuntime<'_>,
        options: &RenderOptions<'_, 'media>,
        image_source: &'p HashMap<String, Arc<usvgr::PreloadedImageData>>,
        size: &VideoSize,
        svgr_images: &'p crate::SvgrImages<'p>,
        fast_shapes: bool,
    ) -> Self {
        Self {
            video,
            ctx: FFramesContext {
                time_base: runtime.time_base,
                mode: crate::FFramesMode::Renderer,
                media_source: options.media,
                duration_in_frames: runtime.timeline.duration_in_frames,
                scenes: runtime.timeline.scenes.as_ref(),
                font_source: Some(&runtime.font_source),
                abort_signal: None,
                current_video_size: size.clone(),
            },
            options: usvgr::Options {
                image_data: Some(image_source),
                font_family: options.default_font.to_string(),
                fast_shapes,
                ..Default::default()
            },
            font_db: runtime.font_source.as_db_ref(),
            svgr_images,
        }
    }

    /// Calls `Video::render_frame` for `frame` and converts the result.
    fn convert(&self, worker: &mut Worker, frame: usize) -> FFramesRendererResult<usvgr::Tree> {
        if frame >= self.ctx.duration_in_frames {
            return Err(FFramesRendererError::Custom(format!(
                "frame {frame} is outside the video (0..{})",
                self.ctx.duration_in_frames
            )));
        }
        let svgr = self.svgr_images.enter(&self.options, self.font_db, || {
            super::render_frame_guarded(
                self.video,
                Frame::__internal_make_for_renderer(
                    frame,
                    frame,
                    TVideo::FPS,
                    worker.text_cache.clone(),
                    worker.decoders.clone(),
                ),
                &self.ctx,
            )
        })?;
        Ok(svgr.into_svg_tree(&self.options, &mut worker.converter_cache, self.font_db)?)
    }
}

/// The CPU rasterizer when the crate has it.
#[cfg_attr(feature = "cpu_renderer", allow(clippy::unnecessary_wraps))]
pub(super) fn default_svgr_rasterizer() -> Option<Box<dyn crate::SvgrRasterizer + 'static>> {
    #[cfg(feature = "cpu_renderer")]
    return Some(Box::new(super::cpu::CpuSvgrRasterizer));
    #[cfg(not(feature = "cpu_renderer"))]
    None
}

/// How [`Previewer::convert_each`] converts frames.
#[derive(Clone, Copy)]
struct Batch {
    /// See [`FrameRenderer::fast_shapes`].
    fast_shapes: bool,
    /// Report the problems found while converting and those of [`diagnostics::inspect_tree`].
    inspect: bool,
    threads: usize,
    schedule: Schedule,
}

/// How the threads of [`Previewer::convert_each`] share the frames.
#[derive(Clone, Copy)]
enum Schedule {
    /// A thread converts the next frame, at most [`AHEAD_PER_THREAD`] frames for each thread
    /// ahead of the frame that `each` needs next. The trees wait in memory until then.
    Ahead,
    /// Each thread converts one contiguous part of the frames. So the video decoders of a
    /// thread read forward, instead of decoding the frames of the other threads too. All
    /// results can wait until `each` gets them, so they must be small.
    Parts,
}

/// How many frames each thread of [`Schedule::Ahead`] may convert ahead of the frame that
/// the renderer needs next.
const AHEAD_PER_THREAD: usize = 2;

/// The threads that convert frames for [`Previewer::render_each`] and the other methods for
/// several frames: a quarter of [`super::get_thread_count`], from one to four.
///
/// The renderer draws one frame at a time, and two to four threads keep it busy. More
/// threads only decode the video sources again: each thread has its own decoders, and with
/// [`Schedule::Ahead`] they decode the frames of the other threads too.
fn convert_threads() -> usize {
    (super::get_thread_count() / 4).clamp(1, 4)
}

/// Runs `work` for the items `0..count` on `threads` new threads and calls `done` on this
/// thread with the results in the order of the items. A thread takes `run` items that follow
/// each other at a time. It takes them only when `done` got all items but the last `window`
/// before them, so at most `window + run - 1` results wait in memory. `worker` makes the
/// state of a thread on that thread. Stops at the first error of `done` and returns it.
fn in_order<W, R: Send, E>(
    count: usize,
    threads: usize,
    run: usize,
    window: usize,
    worker: impl Fn() -> W + Sync,
    work: impl Fn(&mut W, usize) -> R + Sync,
    mut done: impl FnMut(usize, R) -> Result<(), E>,
) -> Result<(), E> {
    use std::sync::{Condvar, Mutex, MutexGuard, PoisonError, mpsc};

    struct Progress {
        /// The next item that a thread starts.
        next: usize,
        /// The number of items that `done` got.
        done: usize,
        stop: bool,
    }

    struct Shared {
        progress: Mutex<Progress>,
        moved: Condvar,
    }

    impl Shared {
        fn lock(&self) -> MutexGuard<'_, Progress> {
            self.progress.lock().unwrap_or_else(PoisonError::into_inner)
        }

        fn stop(&self) {
            self.lock().stop = true;
            self.moved.notify_all();
        }
    }

    /// Stops all threads when the thread that holds it panics, so that no thread waits for
    /// an item that never comes. The scope then panics too.
    struct StopOnPanic<'s>(&'s Shared);

    impl Drop for StopOnPanic<'_> {
        fn drop(&mut self) {
            if std::thread::panicking() {
                self.0.stop();
            }
        }
    }

    let (run, window) = (run.max(1), window.max(1));
    let shared = Shared {
        progress: Mutex::new(Progress {
            next: 0,
            done: 0,
            stop: false,
        }),
        moved: Condvar::new(),
    };
    let (sender, receiver) = mpsc::channel();
    std::thread::scope(|scope| {
        let _stop = StopOnPanic(&shared);
        for _ in 0..threads.max(1) {
            let sender = sender.clone();
            let (shared, worker, work) = (&shared, &worker, &work);
            scope.spawn(move || {
                let _stop = StopOnPanic(shared);
                let mut state = worker();
                'runs: loop {
                    let items = {
                        let mut progress = shared.lock();
                        while !progress.stop
                            && progress.next < count
                            && progress.next >= progress.done + window
                        {
                            progress = shared
                                .moved
                                .wait(progress)
                                .unwrap_or_else(PoisonError::into_inner);
                        }
                        if progress.stop || progress.next >= count {
                            break;
                        }
                        let start = progress.next;
                        progress.next = (start + run).min(count);
                        start..progress.next
                    };
                    for index in items {
                        if shared.lock().stop
                            || sender.send((index, work(&mut state, index))).is_err()
                        {
                            break 'runs;
                        }
                    }
                }
            });
        }
        drop(sender);

        // The results arrive in any order and wait until `done` got the items before them.
        let mut waiting: HashMap<usize, R> = HashMap::new();
        let mut next = 0;
        for (index, result) in &receiver {
            waiting.insert(index, result);
            while let Some(result) = waiting.remove(&next) {
                if let Err(err) = done(next, result) {
                    shared.stop();
                    return Err(err);
                }
                next += 1;
                shared.lock().done = next;
                shared.moved.notify_all();
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use crate::{AudioMap, Duration, Svgr};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Frames that differ, with a text in a missing font, a missing image from frame 20 and
    /// a panic at `panic_at`.
    struct Changing {
        panic_at: Option<usize>,
    }

    impl Video for Changing {
        const FPS: usize = 10;
        const WIDTH: usize = 64;
        const HEIGHT: usize = 32;

        fn duration(&self) -> Duration<'_> {
            Duration::Frames(24)
        }

        fn audio(&self) -> AudioMap<'_> {
            AudioMap::none()
        }

        fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
            assert!(Some(frame.index) != self.panic_at, "boom");
            if frame.index >= 20 {
                let _ = ctx.get_image("missing.png");
            }
            let width = frame.index * 2 + 1;
            crate::svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" width="64" height="32">
                    <rect x="0" y="4" width={width} height="20" fill="#ff8800" />
                    <text x="2" y="30" font-family="No Such Font" font-size="8">"fframes"</text>
                </svg>
            )
        }
    }

    /// Out of order, with a repeated frame.
    const FRAMES: [usize; 9] = [23, 5, 0, 12, 12, 19, 20, 3, 8];

    fn one_by_one(
        video: &Changing,
        frames: &[usize],
    ) -> (Vec<(RgbaFrame, String)>, Option<String>) {
        let mut previewer = Previewer::new(video, &RenderOptions::default()).unwrap();
        let mut renderer = CpuFrameRenderer::default();
        let mut rendered = Vec::new();
        for &frame in frames {
            match previewer.render_inspected(frame, &mut renderer) {
                Ok((pixels, report)) => rendered.push((pixels, format!("{report:?}"))),
                Err(err) => return (rendered, Some(err.to_string())),
            }
        }
        (rendered, None)
    }

    fn batch(video: &Changing, frames: &[usize]) -> (Vec<(RgbaFrame, String)>, Option<String>) {
        let mut previewer = Previewer::new(video, &RenderOptions::default()).unwrap();
        let mut rendered = Vec::new();
        let result = previewer.render_inspected_on(
            frames,
            &mut CpuFrameRenderer::default(),
            4,
            |pixels, report| {
                rendered.push((pixels, format!("{report:?}")));
                Ok(())
            },
        );
        (rendered, result.err().map(|err| err.to_string()))
    }

    fn assert_same(
        (expected, expected_err): (Vec<(RgbaFrame, String)>, Option<String>),
        (actual, actual_err): (Vec<(RgbaFrame, String)>, Option<String>),
    ) {
        assert_eq!(actual.len(), expected.len());
        for ((pixels, report), (expected_pixels, expected_report)) in actual.iter().zip(&expected) {
            assert_eq!(report, expected_report);
            assert_eq!(pixels.pixels, expected_pixels.pixels, "{report}");
        }
        assert_eq!(actual_err, expected_err);
    }

    #[test]
    fn several_threads_render_and_report_like_one_by_one() {
        let video = Changing { panic_at: None };
        let expected = one_by_one(&video, &FRAMES);
        assert_eq!(expected.0.len(), FRAMES.len());
        assert!(expected.0[0].1.contains("MissingMedia"));
        // A warning of the converter, which goes through the logger of the thread.
        assert!(
            expected
                .0
                .iter()
                .all(|(_, report)| report.contains("MissingFont"))
        );
        assert_same(expected, batch(&video, &FRAMES));

        let mut previewer = Previewer::new(&video, &RenderOptions::default()).unwrap();
        let mut renderer = CpuFrameRenderer::default();
        let mut rendered = Vec::new();
        previewer
            .render_each(&FRAMES, &mut renderer, |frame, pixels| {
                rendered.push((frame, pixels.pixels));
                Ok(())
            })
            .unwrap();
        for (&frame, (rendered_frame, pixels)) in FRAMES.iter().zip(rendered) {
            assert_eq!(rendered_frame, frame);
            assert_eq!(
                pixels,
                previewer.render(frame, &mut renderer).unwrap().pixels
            );
        }
    }

    #[test]
    fn several_threads_stop_at_the_first_error_like_one_by_one() {
        let video = Changing { panic_at: Some(12) };
        let expected = one_by_one(&video, &FRAMES);
        assert_eq!(expected.0.len(), 3);
        assert!(expected.1.as_ref().unwrap().contains("boom"));
        assert_same(expected, batch(&video, &FRAMES));

        let outside = [1, 2, 99, 3];
        let expected = one_by_one(&video, &outside);
        assert_eq!(expected.0.len(), 2);
        assert_same(expected, batch(&video, &outside));
    }

    #[test]
    fn several_threads_inspect_like_one_by_one() {
        let video = Changing { panic_at: Some(12) };
        let mut previewer = Previewer::new(&video, &RenderOptions::default()).unwrap();
        let expected: Vec<String> = FRAMES
            .iter()
            .map(|&frame| format!("{:?}", previewer.inspect(frame).unwrap()))
            .collect();
        assert!(expected[3].contains("Panic"));

        let mut previewer = Previewer::new(&video, &RenderOptions::default()).unwrap();
        let mut reports = Vec::new();
        previewer
            .inspect_on(&FRAMES, 4, |report| {
                reports.push(format!("{report:?}"));
                Ok(())
            })
            .unwrap();
        assert_eq!(reports, expected);
    }

    #[test]
    fn results_come_in_order_and_at_most_a_window_ahead() {
        let done = AtomicUsize::new(0);
        let ahead = AtomicUsize::new(0);
        let mut next = 0;
        in_order(
            300,
            6,
            1,
            8,
            || (),
            |(), index| {
                // `done` is at most the count of `in_order`, so this is at most how far ahead
                // the item started.
                ahead.fetch_max(index + 1 - done.load(Ordering::SeqCst), Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_micros((index % 7) as u64 * 30));
                index * 2
            },
            |index, result| {
                assert_eq!((index, result), (next, next * 2));
                next += 1;
                std::thread::sleep(std::time::Duration::from_micros(100));
                done.store(next, Ordering::SeqCst);
                Ok::<_, ()>(())
            },
        )
        .unwrap();
        assert_eq!(next, 300);
        let ahead = ahead.load(Ordering::SeqCst);
        assert!((2..=8).contains(&ahead), "{ahead} items ahead");
    }

    #[test]
    fn runs_of_items_come_in_order() {
        let threads = Mutex::new(vec![None; 100]);
        let mut next = 0;
        in_order(
            100,
            4,
            25,
            100,
            || (),
            |(), index| {
                threads.lock().unwrap()[index] = Some(std::thread::current().id());
                index
            },
            |index, result| {
                assert_eq!((index, result), (next, next));
                next += 1;
                Ok::<_, ()>(())
            },
        )
        .unwrap();
        assert_eq!(next, 100);
        // Each thread converted one part that follows each other.
        let threads = threads.into_inner().unwrap();
        for part in threads.chunks(25) {
            assert!(part.iter().all(|thread| *thread == part[0]));
        }
    }

    #[test]
    fn an_error_stops_the_threads() {
        let started = Mutex::new(Vec::new());
        let mut delivered = Vec::new();
        let result = in_order(
            100,
            4,
            1,
            8,
            || (),
            |(), index| started.lock().unwrap().push(index),
            |index, ()| {
                if index == 10 {
                    return Err(index);
                }
                delivered.push(index);
                Ok(())
            },
        );
        assert_eq!(result, Err(10));
        assert_eq!(delivered, (0..10).collect::<Vec<_>>());
        let started = started.into_inner().unwrap();
        assert!(started.iter().all(|&index| index < 11 + 8), "{started:?}");
    }

    #[test]
    #[should_panic(expected = "a scoped thread panicked")]
    fn a_panic_of_a_thread_does_not_hang() {
        let _ = in_order(
            100,
            4,
            1,
            2,
            || (),
            |(), index| assert!(index != 7, "boom"),
            |_, ()| Ok::<_, ()>(()),
        );
    }

    #[test]
    #[should_panic(expected = "done panics")]
    fn a_panic_of_done_does_not_hang() {
        let _ = in_order(
            100,
            4,
            1,
            2,
            || (),
            |(), _| std::thread::sleep(std::time::Duration::from_millis(1)),
            |index, ()| {
                assert!(index != 3, "done panics");
                Ok::<_, ()>(())
            },
        );
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use crate::{AudioMap, Duration, Svgr};

    struct EmptyVideo;
    impl Video for EmptyVideo {
        const FPS: usize = 30;
        const WIDTH: usize = 1;
        const HEIGHT: usize = 1;
        fn duration(&self) -> Duration<'_> {
            Duration::Frames(2)
        }
        fn audio(&self) -> AudioMap<'_> {
            AudioMap::none()
        }
        fn render_frame<'a>(&'a self, _: Frame, _: &FFramesContext<'a, '_>) -> Svgr<'a> {
            Svgr::empty()
        }
    }

    struct Renderer(Option<usize>);
    impl FrameRenderer for Renderer {
        fn svg_text_cache_capacity(&self) -> Option<usize> {
            self.0
        }
        fn render_tree(
            &mut self,
            _: &usvgr::Tree,
            _: Color,
            width: u32,
            height: u32,
        ) -> FFramesRendererResult<RgbaFrame> {
            Ok(RgbaFrame {
                width,
                height,
                pixels: Vec::new(),
            })
        }
    }

    #[test]
    fn renderer_text_capacity_is_applied_without_recreating_warm_cache() {
        let mut preview = Previewer::new(&EmptyVideo, &RenderOptions::default()).unwrap();
        preview.configure_renderer_cache(&Renderer(Some(100_000)));
        assert_eq!(preview.converter_text_cache_capacity, 100_000);
        // A value carried by the converter must survive another frame's configuration.
        preview.worker.converter_cache.paint.insert(
            "warm".into(),
            usvgr::Paint::Color(usvgr::Color::new_rgb(1, 2, 3)),
        );
        preview.configure_renderer_cache(&Renderer(Some(100_000)));
        assert!(preview.worker.converter_cache.paint.contains_key("warm"));
        preview.configure_renderer_cache(&Renderer(None));
        assert_eq!(preview.converter_text_cache_capacity, 100_000);
        preview.configure_renderer_cache(&Renderer(Some(0)));
        assert_eq!(preview.converter_text_cache_capacity, 0);
        assert!(preview.worker.converter_cache.usvgr_text_cache.is_none());
    }
}
