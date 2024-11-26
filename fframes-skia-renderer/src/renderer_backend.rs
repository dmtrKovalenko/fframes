use crate::{
    render_pipeline::{self, RenderPipelineConfig},
    resource_provider::SkiaFFramesProvider,
};
use fframes::{
    usvgr::{self, WriteOptions},
    AudioTimelineSamples, ResolvedRenderingTimeline, Video,
};
use fframes_renderer::{FFramesRenderBackend, FFramesRendererResult};
use skia_safe::{gpu, svg::Dom, Surface};

pub struct SkiaFFramesRenderer {
    /// The surface to render the video from
    pub(crate) surface: Surface,
    pub(crate) gpu_context: Option<skia_safe::gpu::DirectContext>,
}

impl SkiaFFramesRenderer {
    pub fn new_cpu(surface: Surface) -> Self {
        Self {
            surface,
            gpu_context: None,
        }
    }

    pub fn new_gpu(surface: Surface, gpu_context: Option<gpu::DirectContext>) -> Self {
        Self {
            surface,
            gpu_context,
        }
    }

    #[cfg(feature = "metal")]
    /// Provides default implementation of metal based backend, which is possible to replicate
    /// manually with `new_gpu` method.
    pub fn new_metal(width: usize, height: usize) -> FFramesRendererResult<Self> {
        use metal_rs::{foreign_types::ForeignType, MTLStorageMode, MTLTextureUsage};
        use metal_rs::{Device, MTLPixelFormat, TextureDescriptor};
        use skia_safe::{
            gpu::{backend_render_targets, mtl, SurfaceOrigin},
            ColorType,
        };

        let device = Device::system_default().ok_or_else(|| {
            fframes_renderer::FFramesRendererError::Skia(
                "Failed to create Metal device".to_string(),
            )
        })?;
        let texture_descriptor = TextureDescriptor::new();
        texture_descriptor.set_width(width as u64);
        texture_descriptor.set_height(height as u64);
        texture_descriptor.set_pixel_format(MTLPixelFormat::RGBA8Unorm);
        texture_descriptor.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        texture_descriptor.set_storage_mode(MTLStorageMode::Shared);

        let texture = device.new_texture(&texture_descriptor);
        let texture_info = unsafe { mtl::TextureInfo::new(texture.as_ptr() as mtl::Handle) };

        let command_queue = device.new_command_queue();
        let backend = unsafe {
            mtl::BackendContext::new(
                device.as_ptr() as mtl::Handle,
                command_queue.as_ptr() as mtl::Handle,
            )
        };

        let mut gpu_context = gpu::direct_contexts::make_metal(&backend, None).unwrap();
        let surface = {
            let backend_render_target =
                backend_render_targets::make_mtl((width as i32, height as i32), &texture_info);

            gpu::surfaces::wrap_backend_render_target(
                &mut gpu_context,
                &backend_render_target,
                SurfaceOrigin::TopLeft,
                ColorType::RGBA8888,
                None,
                None,
            )
            .ok_or_else(|| {
                fframes_renderer::FFramesRendererError::Skia(
                    "Failed to wrap backend render target".to_string(),
                )
            })?
        };

        Ok(Self::new_gpu(surface, Some(gpu_context)))
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
            RenderPipelineConfig::default(),
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
