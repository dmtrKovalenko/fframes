use std::path::Path;

use crate::backends::SkiaBackend;
use crate::skia_pipeline;
use crate::skia_pipeline::Pipeline;
pub use crate::skia_pipeline::{SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig};
use fframes::{AudioTimelineSamples, FrameRenderer, ResolvedRenderingTimeline, Video, usvgr};
use fframes::{FFramesRenderBackend, FFramesRendererResult};

#[derive(Clone)]
pub struct SkiaFFramesRenderer<'a, T: SkiaBackend + Sync + Send> {
    pub(crate) pipeline_config: SkiaPipelineConfig,
    pub(crate) backend: &'a T,
}

impl<'a, TSkiaBackend: SkiaBackend> SkiaFFramesRenderer<'a, TSkiaBackend> {
    /// Creates a new Skia render with based on the supported skia rendering backend.
    /// Currently only Vulkan and Metal are supported (also CPU for testing purposes).
    ///
    /// Check `new_metal` and `new_vulkan` methods for more details.
    pub fn new(
        pipeline_config: SkiaPipelineConfig,
        skia_backend_context: &'a TSkiaBackend,
    ) -> Self {
        Self {
            pipeline_config,
            backend: skia_backend_context,
        }
    }
}

impl<TBackend: SkiaBackend> FFramesRenderBackend for SkiaFFramesRenderer<'_, TBackend> {
    /// Skia previews on the same GPU context: shaders and filters look like in the video.
    fn frame_renderer(&self) -> Option<Box<dyn FrameRenderer + '_>> {
        Some(Box::new(crate::SkiaFrameRenderer::new(self.backend)))
    }

    fn render_frame<'a, 'media: 'a, TVideo: Video + Sync + Sized>(
        self,
        frame: fframes::Frame,
        video: &'a TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr::fontdb::Database,
        ctx: fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<Vec<u8>> {
        let mut converter_cache = usvgr::Cache::default();
        let rtree = fframes::render_frame_guarded(video, frame, &ctx)?.into_svg_tree(
            usvg_options,
            &mut converter_cache,
            font_db,
        )?;

        let frame = crate::SkiaFrameRenderer::new(self.backend).render_tree(
            &rtree,
            TVideo::BACKGROUND_COLOR,
            ctx.current_video_size.width as u32,
            ctx.current_video_size.height as u32,
        )?;

        Ok(frame.pixels)
    }

    fn render<'a, 'media: 'a, TVideo: Video + Sync + Sized + Send>(
        self,
        output: impl AsRef<Path>,
        video: &'a TVideo,
        logger: std::sync::Arc<dyn fframes::FFramesLogger>,
        usvg_options: &'a usvgr::Options,
        render_options: &'a fframes::RenderOptions<'a, 'media>,
        font_db: &'a usvgr::fontdb::Database,
        timeline: &'a ResolvedRenderingTimeline<AudioTimelineSamples>,
        ctx: &'a fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<()>
    where
        Self: Sized,
    {
        let background_color = skia_safe::Color::from_argb(
            TVideo::BACKGROUND_COLOR.a,
            TVideo::BACKGROUND_COLOR.r,
            TVideo::BACKGROUND_COLOR.g,
            TVideo::BACKGROUND_COLOR.b,
        );

        skia_pipeline::render(Pipeline {
            background_color,
            ctx,
            font_db,
            logger,
            output: output.as_ref(),
            pipeline_config: self.pipeline_config,
            render_options,
            skia: self.backend,
            timeline,
            usvg_options,
            video,
        })?;

        Ok(())
    }
}
