pub use crate::render_pipeline::SkiaPipelineConfig;
use crate::{render_pipeline, resource_provider::SkiaFFramesProvider};
use fframes::{
    usvgr::{self, WriteOptions},
    AudioTimelineSamples, ResolvedRenderingTimeline, Video,
};
use fframes_renderer::{FFramesRenderBackend, FFramesRendererResult};
use skia_safe::{gpu, surfaces::raster_n32_premul, svg::Dom, Surface};

pub struct SkiaFFramesRenderer {
    /// The surface to render the video from
    pub(crate) surface: Surface,
    pub(crate) gpu_context: Option<skia_safe::gpu::DirectContext>,
    pub(crate) pipeline_config: SkiaPipelineConfig,
}

impl SkiaFFramesRenderer {
    /// Creates a new skia render with an optional gpu context and an abstracted surface.
    /// If you are using metal or vulkan consider using prebuild `new_metal` and `new_vulkan`
    /// constructors for the reasonable defaults.
    pub fn new(
        pipeline_config: SkiaPipelineConfig,
        surface: Surface,
        gpu_context: Option<gpu::DirectContext>,
    ) -> Self {
        Self {
            surface,
            gpu_context,
            pipeline_config,
        }
    }

    /// Creates new CPU based skia renderer. It is not recommended to use it for the final
    /// video rendering, if you are rendering a video on a target without GPU consider
    /// using a built-in CPU renderer.
    ///
    /// Intended for compatibility layer with GPU renderer and/or testing.
    pub fn new_cpu(
        pipeline_config: SkiaPipelineConfig,
        width: usize,
        height: usize,
    ) -> FFramesRendererResult<Self> {
        let surface = raster_n32_premul((width as i32, height as i32)).ok_or_else(|| {
            fframes_renderer::FFramesRendererError::Custom(
                "Failed to create skia surface".to_string(),
            )
        })?;

        Ok(Self {
            surface,
            gpu_context: None,
            pipeline_config,
        })
    }
}

impl FFramesRenderBackend for SkiaFFramesRenderer {
    fn render_frame<'a, 'media: 'a, TVideo: Video + Sync + Sized>(
        self,
        frame: fframes::Frame,
        video: &'a TVideo,
        usvg_options: &usvgr::Options,
        font_db: &usvgr::fontdb::Database,
        ctx: fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<Vec<u8>> {
        let mut surface = self.surface;
        let mut gpu_context = self.gpu_context;

        let image_info = surface.image_info();
        let compute_byte_size = image_info.compute_byte_size(image_info.min_row_bytes());

        let mut pixels = vec![0; compute_byte_size];
        let pixmap = skia_safe::Pixmap::new(&image_info, &mut pixels, image_info.min_row_bytes())
            .ok_or_else(|| {
            fframes_renderer::FFramesRendererError::Custom("Failed to create pixmap".to_string())
        })?;

        let mut converter_cache = usvgr::Cache::default();
        let rtree = video.render_frame(frame, &ctx).into_svg_tree(
            usvg_options,
            &mut converter_cache,
            font_db,
        )?;

        let provider = SkiaFFramesProvider::new(&ctx);
        let raw_svg = rtree.to_string(&WriteOptions::default());

        let dom = Dom::from_str(&raw_svg, provider).unwrap();
        dom.render(surface.canvas());
        if let Some(gpu_context) = gpu_context.as_mut() {
            gpu_context.flush_and_submit();
        }

        let image = { surface.image_snapshot() };
        let result = image.read_pixels_to_pixmap_with_context(
            gpu_context.as_mut(),
            &pixmap,
            (0, 0),
            skia_safe::image::CachingHint::Allow,
        );

        if !result {
            return Err(fframes_renderer::FFramesRendererError::Custom(
                "Failed to read pixels from Skia image".to_string(),
            ));
        }

        Ok(pixels)
    }

    fn render<'a, 'media: 'a, TVideo: Video + Sync + Sized + Send>(
        self,
        output: &'a str,
        video: &'a TVideo,
        logger: std::sync::Arc<dyn fframes_renderer::FFramesLogger>,
        usvg_options: &'a usvgr::Options,
        encoder_options: &'a fframes_renderer::EncoderOptions<'a>,
        font_db: &'a usvgr::fontdb::Database,
        timeline: &'a ResolvedRenderingTimeline<AudioTimelineSamples>,
        ctx: &'a fframes::FFramesContext<'a, 'media>,
    ) -> FFramesRendererResult<()>
    where
        Self: Sized,
    {
        let provider = SkiaFFramesProvider::new(ctx);

        render_pipeline::start(
            self.pipeline_config,
            self,
            provider,
            output,
            video,
            logger,
            usvg_options,
            encoder_options,
            font_db,
            timeline,
            ctx,
        )
    }
}
