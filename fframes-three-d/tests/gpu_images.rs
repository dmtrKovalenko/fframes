//! Textures from images that stay on the GPU (`fframes::wgpu_layer`: svgr images and 3D
//! renders on the Skia Vulkan backend). The GPU makes their mipmap levels. A surface at a
//! low angle shows them like the CPU levels of the same pixels.
#![cfg(feature = "gpu")]
#![allow(clippy::unreadable_literal)]

use std::sync::{Arc, Mutex};

use fframes_three_d::*;

#[allow(dead_code)]
mod common;

const SIZE: u32 = 256;

/// A device of its own, registered as the GPU of a backend.
fn shared_gpu() -> Option<fframes_media::wgpu_layer::SharedGpu> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
    let gpu = fframes_media::wgpu_layer::SharedGpu::new(device, queue, Arc::new(Mutex::new(())));
    fframes_media::wgpu_layer::register_shared_gpu(&gpu);
    Some(gpu)
}

/// Premultiplied stripes and text-like detail: black and white lines one texel wide, a red
/// square, and a half transparent blue band.
fn pixels() -> Vec<u8> {
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            rgba.extend(match (x, y) {
                (64..128, 64..128) => [255, 0, 0, 255],
                (_, 192..224) => [0, 0, 128, 128],
                _ if (x / 2 + y / 3) % 2 == 0 => [255, 255, 255, 255],
                _ => [20, 20, 20, 255],
            });
        }
    }
    rgba
}

/// `rgba` as a texture on `gpu` that the backend would sample in place.
fn layer(
    gpu: &fframes_media::wgpu_layer::SharedGpu,
    rgba: &[u8],
) -> fframes_media::ImageData<'static> {
    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("test image"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: fframes_media::wgpu_layer::WGPU_LAYER_FORMAT,
        usage: fframes_media::wgpu_layer::WGPU_LAYER_USAGE,
        view_formats: &[],
    });
    gpu.queue().write_texture(
        texture.as_image_copy(),
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SIZE * 4),
            rows_per_image: Some(SIZE),
        },
        texture.size(),
    );
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    fframes_media::wgpu_layer::finish_wgpu_layer(&mut encoder, &texture);
    gpu.submit([encoder.finish()]);
    fframes_media::wgpu_layer::wgpu_layer(gpu, &texture)
}

/// The image on a floor seen at a low angle, where the mipmap levels and the anisotropic
/// filter decide what shows.
fn floor(image: &fframes_media::ImageData<'_>) -> (Scene, Camera) {
    let mut scene = Scene::new();
    scene.add(Node::mesh(
        Geometry::plane(4.0, 4.0).rotate_x(-std::f32::consts::FRAC_PI_2),
        Material::unlit(0xffffff).map(Texture::from_image(image)),
    ));
    let camera = Camera::perspective(40.0)
        .at(0.0, 0.6, 2.6)
        .look_at(vec3(0.0, 0.0, -0.5));
    (scene, camera)
}

#[test]
fn images_on_the_gpu_get_the_mipmaps_of_the_cpu() {
    let Some(gpu) = shared_gpu() else {
        eprintln!("no GPU, skipped");
        return;
    };
    let rgba = pixels();
    let on_gpu = layer(&gpu, &rgba);
    assert!(fframes_media::wgpu_layer::resolve_wgpu_layer(&on_gpu.href()).is_some());
    let in_memory = fframes_media::rendered_image(SIZE, SIZE, rgba);

    let renderer = Renderer::new(480, 270).tone_mapping(ToneMapping::Linear);
    let (scene, camera) = floor(&on_gpu);
    // the GPU makes the levels from the texture, the CPU from the pixels it reads back
    let gpu_levels = renderer.render_pixels(&scene, &camera);
    let read_back = renderer.clone().cpu().render_pixels(&scene, &camera);
    let (scene, camera) = floor(&in_memory);
    let cpu_levels = renderer.render_pixels(&scene, &camera);
    let cpu = renderer.clone().cpu().render_pixels(&scene, &camera);
    common::save("gpu_image_levels", &gpu_levels);
    common::save("cpu_image_levels", &cpu);

    let mean = |a: &Pixels, b: &Pixels| {
        let sum: u64 = (a.rgba.iter().zip(&b.rgba))
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        sum as f64 / a.rgba.len() as f64
    };
    assert_eq!(read_back.rgba, cpu.rgba, "the CPU reads the texture back");
    let levels = mean(&gpu_levels, &cpu_levels);
    let parity = mean(&gpu_levels, &cpu);
    eprintln!("GPU levels against uploaded CPU levels {levels:.3}, against the CPU {parity:.3}");
    assert!(levels < 0.1, "the GPU levels differ by {levels} on average");
    // the samplers of the GPU and the CPU differ most on one texel lines at a low angle
    assert!(
        parity < 3.0,
        "the GPU differs from the CPU by {parity} on average"
    );
}
