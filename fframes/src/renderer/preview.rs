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
                    *c = ((*c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
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
    /// Renders `tree` scaled to `width`x`height` over `background`.
    fn render_tree(
        &mut self,
        tree: &usvgr::Tree,
        background: Color,
        width: u32,
        height: u32,
    ) -> FFramesRendererResult<RgbaFrame>;
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
    converter_cache: usvgr::Cache,
    text_cache: Option<TextCache>,
    decoders: VideoDecodersWorker,
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
            converter_cache: usvgr::Cache::new_with_text_cache(10),
            text_cache: TextCache::new(10),
            decoders: VideoDecodersWorker::new(2),
        })
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
                            mix: track.mix,
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

    fn check_frame(&self, frame: usize) -> FFramesRendererResult<()> {
        if frame >= self.timeline_index.duration_in_frames {
            return Err(FFramesRendererError::Custom(format!(
                "frame {frame} is outside the video (0..{})",
                self.timeline_index.duration_in_frames
            )));
        }
        Ok(())
    }

    /// Calls `Video::render_frame` and converts the result.
    pub fn svg_tree(&mut self, frame: usize) -> FFramesRendererResult<usvgr::Tree> {
        self.check_frame(frame)?;

        let ctx = FFramesContext {
            time_base: self.runtime.time_base,
            mode: crate::FFramesMode::Renderer,
            media_source: self.options.media,
            duration_in_frames: self.runtime.timeline.duration_in_frames,
            scenes: self.runtime.timeline.scenes.as_ref(),
            font_source: Some(&self.runtime.font_source),
            abort_signal: None,
            current_video_size: self.size.clone(),
        };

        let svgr = super::render_frame_guarded(
            self.video,
            Frame::__internal_make_for_renderer(
                frame,
                frame,
                TVideo::FPS,
                self.text_cache.clone(),
                self.decoders.clone(),
            ),
            &ctx,
        )?;

        let usvg_options = usvgr::Options {
            image_data: Some(&self.image_source),
            font_family: self.options.default_font.to_string(),
            ..Default::default()
        };

        Ok(svgr.into_svg_tree(
            &usvg_options,
            &mut self.converter_cache,
            self.runtime.font_source.as_db_ref(),
        )?)
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
        let tree = self.svg_tree(frame)?;
        let (width, height) = self.size();
        renderer.render_tree(&tree, TVideo::BACKGROUND_COLOR, width, height)
    }

    /// Renders a frame and reports the problems found while converting it.
    pub fn render_inspected(
        &mut self,
        frame: usize,
        renderer: &mut dyn FrameRenderer,
    ) -> FFramesRendererResult<(RgbaFrame, FrameReport)> {
        self.check_frame(frame)?;
        let (width, height) = self.size();
        let (tree, mut found) = diagnostics::collect(|| self.svg_tree(frame));
        let tree = tree?;
        found.extend(diagnostics::inspect_tree(
            &tree,
            tree.size().width(),
            tree.size().height(),
        ));
        let pixels = renderer.render_tree(&tree, TVideo::BACKGROUND_COLOR, width, height)?;

        Ok((pixels, self.report(frame, found)))
    }

    fn report(&self, frame: usize, found: Vec<Diagnostic>) -> FrameReport {
        FrameReport {
            frame,
            seconds: self.timeline_index.frame_to_seconds(frame),
            scenes: self
                .timeline_index
                .scenes_at(frame)
                .map(|s| s.name.clone())
                .collect(),
            diagnostics: found.into_iter().map(ReportedDiagnostic::from).collect(),
        }
    }

    /// Converts a frame and reports problems in it (missing media or fonts, clipped text,
    /// panics) without rasterizing it.
    pub fn inspect(&mut self, frame: usize) -> FFramesRendererResult<FrameReport> {
        self.check_frame(frame)?;
        let (tree, mut found) = diagnostics::collect(|| self.svg_tree(frame));

        match tree {
            Ok(tree) => found.extend(diagnostics::inspect_tree(
                &tree,
                tree.size().width(),
                tree.size().height(),
            )),
            // The panic is already reported as a diagnostic.
            Err(FFramesRendererError::FramePanicked(_)) => {}
            Err(err) => return Err(err),
        }

        Ok(self.report(frame, found))
    }
}
