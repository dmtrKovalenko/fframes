//! Skia draws the 3D directly from the GPU texture (`fframes::wgpu_layer`). This test compares
//! these frames with frames that rasterize the 3D on the CPU.
#![cfg(not(target_os = "macos"))]
use arm_example::{ArmMedia, FPS, HEIGHT, Show, WIDTH};
use fframes::{
    CombinedMediaProvider, MediaDirectory, MediaProvider, Previewer, RenderOptions,
    StaticMediaProvider,
};
use fframes_skia_renderer::SkiaFrameRenderer;
use fframes_skia_renderer::vulkan::SkiaVulkanCtx;

#[test]
fn texture_frames_match_cpu_frames() {
    let Ok(ctx) = SkiaVulkanCtx::new(WIDTH, HEIGHT) else {
        eprintln!("no Vulkan device, skipped");
        return;
    };
    assert!(ctx.shared_gpu().is_some(), "wgpu can not use the device");
    let media = ArmMedia::prepare().unwrap();
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dynamic_media");
    let Ok(dir) = MediaDirectory::read_folder(&folder) else {
        eprintln!("no dynamic_media (run arm-soundtrack), skipped");
        return;
    };
    let sounds = dir.process_media_source().unwrap();
    let all = CombinedMediaProvider::from([&media as &dyn MediaProvider, &sounds]);
    let options = RenderOptions {
        media: Some(&all),
        ..Default::default()
    };
    let gpu = Show::new();
    let read_back = Show::new().read_back();
    let cpu = Show::new().on_cpu();
    assert!(
        gpu.image(8.0).href().id.starts_with("fframes-wgpu:"),
        "the 3D is not a GPU layer"
    );
    let mut gpu_preview = Previewer::new(&gpu, &options).unwrap();
    let mut read_back_preview = Previewer::new(&read_back, &options).unwrap();
    let mut cpu_preview = Previewer::new(&cpu, &options).unwrap();
    let mut renderer = SkiaFrameRenderer::new(&ctx);
    let dir = std::env::temp_dir().join("arm-shared-gpu");
    std::fs::create_dir_all(&dir).unwrap();
    for t in [0.6f32, 4.3, 8.0, 17.6, 34.1] {
        let frame = (t * FPS as f32) as usize;
        let start = std::time::Instant::now();
        let a = gpu_preview.render(frame, &mut renderer).unwrap();
        let texture_time = start.elapsed();
        let start = std::time::Instant::now();
        let r = read_back_preview.render(frame, &mut renderer).unwrap();
        let read_back_time = start.elapsed();
        let b = cpu_preview.render(frame, &mut renderer).unwrap();
        // The same GPU image, sampled in place or read back and uploaded again.
        assert_eq!(
            a.pixels, r.pixels,
            "{t}: the texture differs from its readback"
        );
        let diff = a
            .pixels
            .iter()
            .zip(&b.pixels)
            .map(|(x, y)| f64::from((i16::from(*x) - i16::from(*y)).unsigned_abs()))
            .sum::<f64>()
            / a.pixels.len() as f64;
        eprintln!(
            "{t:>5}s  texture {texture_time:>9.2?}  readback {read_back_time:>9.2?}  against the CPU 3D {diff:.2}"
        );
        for (name, f) in [("texture", &a), ("cpu", &b)] {
            image::save_buffer(
                dir.join(format!("{t}_{name}.png")),
                &f.pixels,
                f.width,
                f.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        assert!(diff < 3.0, "{t}: {diff}");
    }
}
