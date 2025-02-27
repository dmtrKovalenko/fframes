use crate::skia_backend::{SkiaFFramesRenderer, SkiaPipelineConfig};
use fframes_renderer::FFramesRendererResult;
use skia_safe::gpu;

impl SkiaFFramesRenderer {
    #[cfg(feature = "metal")]
    /// Provides default implementation of metal based backend, which is possible to replicate
    /// manually with `new_gpu` method.
    pub fn new_metal(
        pipeline_config: SkiaPipelineConfig,
        width: usize,
        height: usize,
    ) -> FFramesRendererResult<Self> {
        use metal_rs::{Device, MTLPixelFormat, TextureDescriptor};
        use metal_rs::{MTLStorageMode, MTLTextureUsage, foreign_types::ForeignType};
        use skia_safe::{
            ColorType,
            gpu::{SurfaceOrigin, backend_render_targets, mtl},
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

        Ok(Self::new(pipeline_config, surface, Some(gpu_context)))
    }
}
