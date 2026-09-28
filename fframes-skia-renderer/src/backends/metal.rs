use super::SkiaBackend;
use crate::skia_backend::{SkiaFFramesRenderer, SkiaPipelineConfig};
use fframes::{FFramesRendererError, FFramesRendererResult};
use foreign_types_shared::ForeignType;
pub use metal_rs;
use metal_rs::{Device, MTLPixelFormat, TextureDescriptor};
use metal_rs::{MTLStorageMode, MTLTextureUsage};
use skia_safe::gpu::DirectContext;
use skia_safe::gpu::ganesh::context_options::{Enable, ShaderCacheStrategy};
use skia_safe::{
    ColorType,
    gpu::{SurfaceOrigin, backend_render_targets, mtl},
};
use skia_safe::{Surface, gpu};

#[allow(dead_code)]
pub struct SkiaMetalCtx {
    device: metal_rs::Device,
    command_queue: metal_rs::CommandQueue,
    width: usize,
    height: usize,
    backend: mtl::BackendContext,
}

unsafe impl Send for SkiaMetalCtx {}
unsafe impl Sync for SkiaMetalCtx {}

impl SkiaMetalCtx {
    pub fn new(width: usize, height: usize) -> FFramesRendererResult<Self> {
        let device = Device::system_default().ok_or_else(|| {
            FFramesRendererError::Skia("Failed to create Metal device".to_string())
        })?;
        let command_queue = device.new_command_queue();

        Self::new_with_device(device, command_queue, width, height)
    }

    pub fn new_with_device(
        device: Device,
        command_queue: metal_rs::CommandQueue,
        width: usize,
        height: usize,
    ) -> FFramesRendererResult<Self> {
        let backend = unsafe {
            mtl::BackendContext::new(
                device.as_ptr() as mtl::Handle,
                command_queue.as_ptr() as mtl::Handle,
            )
        };

        Ok(SkiaMetalCtx {
            device,
            command_queue,
            width,
            height,
            backend,
        })
    }

    /// Create skia context for the existing texture, make sure
    /// that the texture should be living longer than this context
    pub fn new_from_existing_texture(
        device_ptr: *mut std::ffi::c_void,
        command_queue_ptr: *mut std::ffi::c_void,
        texture_ptr: *mut std::ffi::c_void,
        color_type: ColorType,
        width: usize,
        height: usize,
    ) -> FFramesRendererResult<(Self, Surface, DirectContext)> {
        let device = unsafe { metal_rs::Device::from_ptr(device_ptr as *mut _) };
        let command_queue =
            unsafe { metal_rs::CommandQueue::from_ptr(command_queue_ptr as *mut _) };
        let texture = unsafe { metal_rs::Texture::from_ptr(texture_ptr as *mut _) };
        let texture_info = unsafe { mtl::TextureInfo::new(texture.as_ptr() as mtl::Handle) };

        let backend = unsafe {
            mtl::BackendContext::new(
                device.as_ptr() as mtl::Handle,
                command_queue.as_ptr() as mtl::Handle,
            )
        };

        let mut gpu_context =
            gpu::direct_contexts::make_metal(&backend, Some(&gpu_context_options())).unwrap();
        let ctx = Self::new_with_device(device, command_queue, width, height)?;
        let surface = {
            let backend_render_target =
                backend_render_targets::make_mtl((width as i32, height as i32), &texture_info);

            gpu::surfaces::wrap_backend_render_target(
                &mut gpu_context,
                &backend_render_target,
                SurfaceOrigin::TopLeft,
                color_type,
                None,
                None,
            )
            .ok_or_else(|| {
                FFramesRendererError::Skia("Failed to wrap backend render target".to_string())
            })
        }?;

        Ok((ctx, surface, gpu_context))
    }
}

/// Skia GPU context settings, the same as the Vulkan backend uses.
fn gpu_context_options() -> gpu::ContextOptions {
    let mut gpu_context_opts = gpu::ContextOptions::new();

    // Cache configuration
    gpu_context_opts.glyph_cache_texture_maximum_bytes = 64 * 1024 * 1024; // 64MB
    gpu_context_opts.allow_multiple_glyph_cache_textures = Enable::Yes;
    gpu_context_opts.buffer_map_threshold = 4096;
    gpu_context_opts.minimum_staging_buffer_size = 1_048_576;

    // Path rendering optimizations
    gpu_context_opts.allow_path_mask_caching = true;
    gpu_context_opts.disable_distance_field_paths = false;
    gpu_context_opts.disable_coverage_counting_paths = true;

    // Shader configuration
    gpu_context_opts.runtime_program_cache_size = 256;
    gpu_context_opts.shader_cache_strategy = ShaderCacheStrategy::BackendBinary;
    gpu_context_opts.reduced_shader_variations = false;

    // Batch processing
    gpu_context_opts.reduce_ops_task_splitting = Enable::Yes;

    gpu_context_opts
}

impl SkiaBackend for SkiaMetalCtx {
    fn create_skia_surface(&self) -> FFramesRendererResult<(Surface, Option<DirectContext>)> {
        let mut gpu_context =
            gpu::direct_contexts::make_metal(&self.backend, Some(&gpu_context_options())).unwrap();

        let texture_descriptor = TextureDescriptor::new();
        texture_descriptor.set_width(self.width as u64);
        texture_descriptor.set_height(self.height as u64);
        texture_descriptor.set_pixel_format(MTLPixelFormat::RGBA8Unorm);
        texture_descriptor.set_usage(
            MTLTextureUsage::RenderTarget
                | MTLTextureUsage::ShaderRead
                | MTLTextureUsage::ShaderWrite,
        );
        texture_descriptor.set_storage_mode(MTLStorageMode::Shared);

        let texture = self.device.new_texture(&texture_descriptor);
        let texture_info = unsafe { mtl::TextureInfo::new(texture.as_ptr() as mtl::Handle) };

        let surface = {
            let backend_render_target = backend_render_targets::make_mtl(
                (self.width as i32, self.height as i32),
                &texture_info,
            );

            gpu::surfaces::wrap_backend_render_target(
                &mut gpu_context,
                &backend_render_target,
                SurfaceOrigin::TopLeft,
                ColorType::RGBA8888,
                None,
                None,
            )
            .ok_or_else(|| {
                FFramesRendererError::Skia("Failed to wrap backend render target".to_string())
            })?
        };

        Ok((surface, Some(gpu_context)))
    }
}

impl<'a> SkiaFFramesRenderer<'a, SkiaMetalCtx> {
    #[cfg(feature = "metal")]
    /// Provides default implementation of metal based backend, which is possible to replicate
    /// manually with `new_gpu` method.
    pub fn new_metal(
        ctx: &'a SkiaMetalCtx,
        pipeline_config: SkiaPipelineConfig,
    ) -> FFramesRendererResult<Self> {
        Ok(Self::new(pipeline_config, ctx))
    }
}
