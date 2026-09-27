use std::path::Path;

use crate::backends::SkiaBackend;
use crate::skia_pipeline;
use crate::skia_pipeline::Pipeline;
pub use crate::skia_pipeline::{SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig};
use fframes::{AudioTimelineSamples, ResolvedRenderingTimeline, Video, usvgr};
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
    fn render_frame<'a, 'media: 'a, TVideo: Video + Sync + Sized>(
        self,
        frame: fframes::Frame,
        video: &'a TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr::fontdb::Database,
        ctx: fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<Vec<u8>> {
        let (mut surface, mut gpu_context) = self.backend.create_skia_surface()?;
        let image_info = surface.image_info();
        let frame_datavec_size = image_info.compute_byte_size(image_info.min_row_bytes());

        let mut pixels = vec![0; frame_datavec_size];
        let pixmap = skia_safe::Pixmap::new(&image_info, &mut pixels, image_info.min_row_bytes())
            .ok_or_else(|| {
            fframes::FFramesRendererError::Custom("Failed to create pixmap".to_string())
        })?;

        let mut converter_cache = usvgr::Cache::default();
        let rtree = video.render_frame(frame, &ctx).into_svg_tree(
            usvg_options,
            &mut converter_cache,
            font_db,
        )?;

        let mut render_cache = crate::render::RenderCache::new();
        crate::render::render_tree(&rtree, surface.canvas(), &mut render_cache);

        if let Some(gpu_context) = gpu_context.as_mut() {
            gpu_context.flush_and_submit();
        }

        let image = surface.image_snapshot();
        let result = image.read_pixels_to_pixmap_with_context(
            gpu_context.as_mut(),
            &pixmap,
            (0, 0),
            skia_safe::image::CachingHint::Allow,
        );

        if !result {
            return Err(fframes::FFramesRendererError::Custom(
                "Failed to read pixels from Skia image".to_string(),
            ));
        }

        Ok(pixels)
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
