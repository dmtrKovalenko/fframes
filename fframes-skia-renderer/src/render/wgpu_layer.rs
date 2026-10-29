//! Textures of `fframes::wgpu_layer`s, sampled in place (feature `wgpu`).
//!
//! Skia borrows the texture in the sampled layout that wgpu left it in
//! (`fframes::finish_wgpu_layer`), so Skia records no barrier. The transition at the end of
//! the layer's submission orders the layer before every later submission on the queue.

use ash::vk::{self, Handle};
use skia_safe::gpu::{self, SurfaceOrigin, ganesh::vk::backend_textures};
use skia_safe::{AlphaType, Canvas, ColorType, Image};

/// The layer's texture as an image of the canvas's GPU context. `None` when the canvas
/// records a picture, or does not draw on the layer's device (`shared_gpu` is the device
/// of the context). Read the pixels back then.
pub(super) fn image(
    layer: &fframes::WgpuLayer,
    canvas: &Canvas,
    shared_gpu: Option<u64>,
) -> Option<Image> {
    if shared_gpu != Some(layer.gpu_id()) {
        return None;
    }
    let mut context = canvas.recording_context()?;
    let backend_texture = backend_texture(
        layer.texture(),
        gpu::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    )?;
    gpu::images::borrow_texture_from(
        &mut context,
        &backend_texture,
        SurfaceOrigin::TopLeft,
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    )
}

/// `texture` as a Skia texture in `layout`. `None` when the texture is not a Vulkan texture.
pub(crate) fn backend_texture(
    texture: &wgpu::Texture,
    layout: gpu::vk::ImageLayout,
) -> Option<gpu::BackendTexture> {
    let raw = {
        let hal = unsafe { texture.as_hal::<wgpu::hal::api::Vulkan>() }?;
        unsafe { hal.raw_handle() }
    };
    let mut info = unsafe {
        gpu::vk::ImageInfo::new(
            raw.as_raw() as _,
            gpu::vk::Alloc::default(),
            gpu::vk::ImageTiling::OPTIMAL,
            layout,
            gpu::vk::Format::R8G8B8A8_UNORM,
            1,
            None,
            None,
            gpu::Protected::No,
            None,
        )
    };
    info.image_usage_flags = usage_flags(texture.usage()).as_raw();
    info.sample_count = 1;
    Some(unsafe {
        backend_textures::make_vk(
            (texture.width() as i32, texture.height() as i32),
            &info,
            "fframes wgpu texture",
        )
    })
}

fn usage_flags(usage: wgpu::TextureUsages) -> vk::ImageUsageFlags {
    let mut flags = vk::ImageUsageFlags::empty();
    for (from, to) in [
        (
            wgpu::TextureUsages::COPY_SRC,
            vk::ImageUsageFlags::TRANSFER_SRC,
        ),
        (
            wgpu::TextureUsages::COPY_DST,
            vk::ImageUsageFlags::TRANSFER_DST,
        ),
        (
            wgpu::TextureUsages::TEXTURE_BINDING,
            vk::ImageUsageFlags::SAMPLED,
        ),
        (
            wgpu::TextureUsages::STORAGE_BINDING,
            vk::ImageUsageFlags::STORAGE,
        ),
        (
            wgpu::TextureUsages::RENDER_ATTACHMENT,
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
        ),
    ] {
        if usage.contains(from) {
            flags |= to;
        }
    }
    flags
}
