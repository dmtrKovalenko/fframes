//! Skia draws textures rendered with wgpu on its own device in place.
#![cfg(feature = "wgpu")]

use fframes::{usvgr, wgpu};
use fframes_skia_renderer::render::{RenderCache, render_tree};
use fframes_skia_renderer::vulkan::SkiaVulkanCtx;
use fframes_skia_renderer::{SkiaBackend, lock_queue};
use skia_safe::{AlphaType, ColorType, ImageInfo};

const SIZE: u32 = 64;

/// Clears the layer texture to `color` (premultiplied, as stored).
fn fill(gpu: &fframes::SharedGpu, texture: &wgpu::Texture, color: [f64; 4]) {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: color[0],
                    g: color[1],
                    b: color[2],
                    a: color[3],
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    fframes::finish_wgpu_layer(&mut encoder, texture);
    gpu.submit([encoder.finish()]);
}

/// Draws `layer` over white with `cache`. Returns the center pixel.
fn draw(
    ctx: &SkiaVulkanCtx,
    layer: &fframes::media::ImageData<'static>,
    cache: &mut RenderCache,
) -> [u8; 4] {
    let tree = fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
            <image href={layer.href()} x="0" y="0" width="64" height="64" />
        </svg>
    )
    .into_svg_tree(
        &usvgr::Options::default(),
        &mut usvgr::Cache::default(),
        &usvgr::fontdb::Database::new(),
    )
    .unwrap();
    let mut context = ctx.create_skia_context().unwrap();
    context.surface.canvas().clear(skia_safe::Color::WHITE);
    render_tree(&tree, context.surface.canvas(), cache);
    let info = ImageInfo::new(
        (SIZE as i32, SIZE as i32),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
    {
        let _queue = lock_queue(context.queue_lock.as_ref());
        assert!(
            context
                .surface
                .read_pixels(&info, &mut pixels, SIZE as usize * 4, (0, 0))
        );
    }
    drop(tree);
    let queue_lock = context.queue_lock.clone();
    let _queue = lock_queue(queue_lock.as_ref());
    drop(context);
    let i = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn assert_near(actual: [u8; 4], expected: [u8; 4]) {
    let close = actual
        .iter()
        .zip(expected)
        .all(|(a, e)| (i32::from(*a) - i32::from(e)).abs() <= 1);
    assert!(close, "expected {expected:?}, got {actual:?}");
}

#[test]
fn layers_are_sampled_in_place_and_reused() {
    let Ok(ctx) = SkiaVulkanCtx::new(SIZE as usize, SIZE as usize) else {
        eprintln!("no Vulkan device, skipped");
        return;
    };
    let gpu = ctx.shared_gpu().expect("wgpu opens the device").clone();
    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("test layer"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: fframes::WGPU_LAYER_FORMAT,
        usage: fframes::WGPU_LAYER_USAGE | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let mut cache = RenderCache::new().for_backend(&ctx);

    // opaque orange, then the same texture with half transparent blue: Skia draws the
    // last render, blended over white
    fill(&gpu, &texture, [1.0, 0.5, 0.0, 1.0]);
    let layer = fframes::wgpu_layer(&gpu, &texture);
    assert_near(draw(&ctx, &layer, &mut cache), [255, 128, 0, 255]);
    drop(layer);

    fill(&gpu, &texture, [0.0, 0.0, 0.5, 0.5]);
    let layer = fframes::wgpu_layer(&gpu, &texture);
    assert_near(draw(&ctx, &layer, &mut cache), [128, 128, 255, 255]);

    // a context on another device reads the texture back
    assert_near(
        draw(&ctx, &layer, &mut RenderCache::new()),
        [128, 128, 255, 255],
    );
    drop(layer);

    // an sRGB view changes the colors, and Skia needs the copy usages: not a layer
    let wrong = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let layer = fframes::wgpu_layer(&gpu, &wrong);
    assert!(fframes::resolve_wgpu_layer(&layer.href()).is_none());
}

/// A fframes-3d frame on the shared GPU is a layer, and Skia draws the same picture as the
/// pixels that the renderer reads back.
#[test]
fn fframes_3d_frames_are_layers() {
    use fframes_3d::{Background, Camera, Geometry, Material, Node, Renderer, Rgb, Scene};

    let Ok(ctx) = SkiaVulkanCtx::new(SIZE as usize, SIZE as usize) else {
        eprintln!("no Vulkan device, skipped");
        return;
    };
    assert!(ctx.shared_gpu().is_some(), "wgpu opens the device");
    let mut scene = Scene::new().background(Background::Color(Rgb::from("#2040ff")));
    scene.add(Node::mesh(
        Geometry::cuboid(1.0, 1.0, 1.0),
        Material::unlit("#ff8000"),
    ));
    let camera = Camera::perspective(40.0).at(0.0, 0.0, 4.0);
    let renderer = Renderer::new(SIZE, SIZE).gpu();

    let layer = renderer.render(&scene, &camera);
    assert!(fframes::resolve_wgpu_layer(&layer.href()).is_some());
    let drawn = draw(&ctx, &layer, &mut RenderCache::new().for_backend(&ctx));
    let expected = renderer.render_pixels(&scene, &camera);
    let center = (SIZE / 2) as usize * SIZE as usize + (SIZE / 2) as usize;
    let expected = expected.rgba.as_chunks::<4>().0[center];
    assert_near(drawn, expected);
}
