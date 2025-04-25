use crate::{SkiaBackend, SkiaFFramesRenderer, resource_provider::SkiaFFramesProvider};
use fframes::{
    FFramesContext, FFramesRendererError, FFramesRendererResult, FFramesRendererRuntime,
    MediaProvider, TextCache, TimeBase, Video, VideoDecodersWorker, usvgr,
};
use skia_safe::{Surface, svg::Dom};
use std::sync::Mutex;

/// Creates a new GPU context that can be used for instant rendering.
/// Recommended for use with externally provided textures to minimize the copying overhead.
pub struct InstantRenderingGPUBackend<TBackend: SkiaBackend> {
    #[allow(dead_code)] // this is required for lifetime in case of ffi usage
    backend: TBackend,
    surface: Surface,
    gpu_context: skia_safe::gpu::DirectContext,
}

impl<TBackend: SkiaBackend> InstantRenderingGPUBackend<TBackend> {
    pub fn new(backend: TBackend) -> FFramesRendererResult<Self> {
        let (surface, gpu_context) = backend.create_skia_surface().unwrap();

        let gpu_context = gpu_context.ok_or_else(|| {
            FFramesRendererError::Skia(
                "Instant Rendering is only available for gpu renderers".to_string(),
            )
        })?;

        Ok(Self {
            backend,
            surface,
            gpu_context,
        })
    }

    pub fn new_from_existing_texture(
        backend: TBackend,
        surface: Surface,
        gpu_context: skia_safe::gpu::DirectContext,
    ) -> Self {
        Self {
            backend,
            surface,
            gpu_context,
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
    converter_cache: Mutex<usvgr::Cache>,
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
            ..Default::default()
        };

        let break_lines_cache = TextCache::new(1000);
        let converter_cache = Mutex::new(usvgr::Cache::default());

        Ok(Self {
            runtime,
            usvg_options,
            break_lines_cache,
            converter_cache,
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

        let provider = SkiaFFramesProvider::new(fframes_ctx.video_decoders.clone(), &ctx);
        let fframe = fframes::Frame::__internal_make_for_renderer(
            frame_index,
            frame_index,
            ctx.time_base.fps,
            fframes_ctx.break_lines_cache.clone(),
            provider.worker_local_decoders.clone(),
        );

        let mut converter_cache = fframes_ctx.converter_cache.lock().unwrap();
        let svg = {
            video
                .render_frame(fframe, &ctx)
                .into_svg_tree(
                    &fframes_ctx.usvg_options,
                    &mut converter_cache,
                    fframes_ctx.runtime.font_source.as_db_ref(),
                )?
                .to_string(&usvgr::WriteOptions::default())
        };

        native_ctx.surface.canvas().clear(skia_safe::Color::BLACK);
        let dom = Dom::from_str(&svg, provider.clone())
            .map_err(|_| FFramesRendererError::Custom("Failed to create dom".to_string()))?;

        dom.render(native_ctx.surface.canvas());
        native_ctx.gpu_context.flush_and_submit();

        Ok(frame_index < fframes_ctx.runtime.timeline.duration_in_frames - 1)
    }
}
