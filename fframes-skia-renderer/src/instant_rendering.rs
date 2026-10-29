use crate::{SkiaBackend, SkiaFFramesRenderer};
use fframes::{
    FFramesContext, FFramesRendererError, FFramesRendererResult, FFramesRendererRuntime,
    MediaProvider, TextCache, TimeBase, Video, VideoDecodersWorker, usvgr,
};
use skia_safe::Surface;
use std::mem::ManuallyDrop;

/// Creates a new GPU context that can be used for instant rendering.
/// Recommended for use with externally provided textures to minimize the copying overhead.
///
/// Each `InstantRenderingGPUBackend` owns its own render caches, so multiple backends can
/// render frames concurrently without contention.
pub struct InstantRenderingGPUBackend<TBackend: SkiaBackend> {
    render_cache: crate::render::RenderCache,
    converter_cache: usvgr::Cache,
    // Skia submits when these go, so `Drop` drops them under the queue lock
    surface: ManuallyDrop<Surface>,
    gpu_context: ManuallyDrop<skia_safe::gpu::DirectContext>,
    /// Held around every call that reaches the queue, see [`crate::SkiaContext::queue_lock`].
    queue_lock: Option<crate::QueueLock>,
    // Last: the device of the backend outlives everything made on it.
    #[allow(dead_code)] // this is required for lifetime in case of ffi usage
    backend: TBackend,
}

impl<TBackend: SkiaBackend> InstantRenderingGPUBackend<TBackend> {
    pub fn new(backend: TBackend) -> FFramesRendererResult<Self> {
        let context = backend.create_skia_context()?;

        let gpu_context = context.gpu.ok_or_else(|| {
            FFramesRendererError::Skia(
                "Instant Rendering is only available for gpu renderers".to_string(),
            )
        })?;

        Ok(Self {
            render_cache: crate::render::RenderCache::new().for_backend(&backend),
            converter_cache: usvgr::Cache::default(),
            surface: ManuallyDrop::new(context.surface),
            gpu_context: ManuallyDrop::new(gpu_context),
            queue_lock: context.queue_lock,
            backend,
        })
    }

    /// Applies cache limits to instant rendering and clears cached resources.
    pub fn with_cache_config(mut self, config: crate::SkiaCacheConfig) -> Self {
        self.converter_cache = usvgr::Cache::new_with_text_cache(config.text_capacity);
        self.render_cache = self.render_cache.cleared(config);
        self
    }

    /// Renders into `surface` of `gpu_context`. They must submit to a queue that nothing
    /// else uses at the same time.
    pub fn new_from_existing_texture(
        backend: TBackend,
        surface: Surface,
        gpu_context: skia_safe::gpu::DirectContext,
    ) -> Self {
        Self {
            render_cache: crate::render::RenderCache::new().for_backend(&backend),
            converter_cache: usvgr::Cache::default(),
            surface: ManuallyDrop::new(surface),
            gpu_context: ManuallyDrop::new(gpu_context),
            queue_lock: None,
            backend,
        }
    }
}

impl<TBackend: SkiaBackend> Drop for InstantRenderingGPUBackend<TBackend> {
    fn drop(&mut self) {
        let _queue = crate::lock_queue(self.queue_lock.as_ref());
        self.render_cache = crate::render::RenderCache::new();
        // SAFETY: dropped once, here; the fields are not used after this
        unsafe {
            ManuallyDrop::drop(&mut self.surface);
            ManuallyDrop::drop(&mut self.gpu_context);
        }
    }
}

/// This represents the runtime context for fframes used in instant rendering.
/// Make sure that it includes the resolved timeline, which indicates how the video might be
/// constructed (for example, the duration based on the audio). It is your responsibility
/// to update this context whenever there are timeline-sensitive changes made to the video structure.
pub struct InstantRenderingVideoCtx<'a> {
    pub runtime: fframes::FFramesRendererRuntime<'a>,
    usvg_options: usvgr::Options<'a>,
    break_lines_cache: Option<TextCache>,
    video_decoders: VideoDecodersWorker,
}

impl InstantRenderingVideoCtx<'_> {
    pub fn new<TVideo: Video>(
        video: &'static TVideo,
        media: Option<&'static dyn MediaProvider<'static>>,
    ) -> FFramesRendererResult<Self> {
        let scenes = video.define_scenes();
        let runtime = FFramesRendererRuntime::new(
            TimeBase {
                fps: TVideo::FPS,
                sample_rate: 44100,
            },
            video,
            &scenes,
            media,
        )?;

        let usvg_options = usvgr::Options {
            font_family: "Arial".to_string(),
            fast_shapes: true,
            ..Default::default()
        };

        let break_lines_cache = TextCache::new(1000);

        Ok(Self {
            runtime,
            usvg_options,
            break_lines_cache,
            video_decoders: VideoDecodersWorker::new(1),
        })
    }
}

impl<TBackend: SkiaBackend> SkiaFFramesRenderer<'_, TBackend> {
    /// Instantly renders the frame to the provided GPU surface
    /// can be used to integrate with a canvas for a native rendering.
    ///
    /// @return true if there are more frames to render false otherwise or if the frame index is
    /// out of bounds. Error is returned only if the rendering itself fails
    pub fn instant_render<'a, 'media: 'a, TVideo: fframes::Video>(
        frame_index: usize,
        video: &'a TVideo,
        media: Option<&'media dyn fframes::MediaProvider<'media>>,
        fframes_ctx: &InstantRenderingVideoCtx<'a>,
        native_ctx: &mut InstantRenderingGPUBackend<TBackend>,
    ) -> FFramesRendererResult<bool> {
        if frame_index >= fframes_ctx.runtime.timeline.duration_in_frames {
            return Ok(false);
        }

        let ctx = FFramesContext {
            time_base: fframes_ctx.runtime.time_base,
            current_video_size: fframes::VideoSize {
                width: TVideo::WIDTH,
                height: TVideo::HEIGHT,
            },
            abort_signal: None,
            duration_in_frames: fframes_ctx.runtime.timeline.duration_in_frames,
            mode: fframes::FFramesMode::Editor,
            scenes: fframes_ctx.runtime.timeline.scenes.as_ref(),
            media_source: media,
            font_source: Some(&fframes_ctx.runtime.font_source),
        };

        let fframe = fframes::Frame::__internal_make_for_renderer(
            frame_index,
            frame_index,
            ctx.time_base.fps,
            fframes_ctx.break_lines_cache.clone(),
            fframes_ctx.video_decoders.clone(),
        );

        let tree = fframes::render_frame_guarded(video, fframe, &ctx)?.into_svg_tree(
            &fframes_ctx.usvg_options,
            &mut native_ctx.converter_cache,
            fframes_ctx.runtime.font_source.as_db_ref(),
        )?;

        let background = TVideo::BACKGROUND_COLOR;
        native_ctx
            .surface
            .canvas()
            .clear(skia_safe::Color::from_argb(
                background.a,
                background.r,
                background.g,
                background.b,
            ));
        crate::render::render_tree(
            &tree,
            native_ctx.surface.canvas(),
            &mut native_ctx.render_cache,
        );
        let _queue = crate::lock_queue(native_ctx.queue_lock.as_ref());
        native_ctx.gpu_context.flush_and_submit();

        Ok(frame_index < fframes_ctx.runtime.timeline.duration_in_frames - 1)
    }
}
