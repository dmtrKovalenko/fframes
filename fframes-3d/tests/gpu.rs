//! Compares the GPU renderer with the CPU rasterizer. It needs the `gpu` feature and skips
//! when no GPU is available.
#![cfg(feature = "gpu")]
#![allow(clippy::unreadable_literal)]

use fframes_3d::*;

mod common;

/// Saves both images and returns their mean difference in 8-bit steps.
fn compare(name: &str, cpu: &Pixels, gpu: &Pixels) -> f32 {
    let diff: f64 = cpu
        .rgba
        .iter()
        .zip(&gpu.rgba)
        .map(|(a, b)| f64::from(a.abs_diff(*b)))
        .sum();
    let mean = (diff / cpu.rgba.len() as f64) as f32;
    common::save(&format!("{name}_cpu"), cpu);
    common::save(&format!("{name}_gpu"), gpu);
    eprintln!("{name}: mean difference {mean:.2} / 255");
    mean
}

#[test]
fn gpu_matches_cpu() {
    let gpu_renderer = Renderer::new(960, 540).bloom(Bloom::default());
    if !gpu_renderer.uses_gpu() {
        eprintln!("no GPU, skipped");
        return;
    }
    let cpu_renderer = gpu_renderer.clone().cpu();
    let (scene, camera) = common::showcase();
    let cpu = cpu_renderer.render_pixels(&scene, &camera);
    let start = std::time::Instant::now();
    let mut gpu = gpu_renderer.render_pixels(&scene, &camera);
    eprintln!("first gpu frame {:?}", start.elapsed());
    let start = std::time::Instant::now();
    for _ in 0..10 {
        gpu = gpu_renderer.render_pixels(&scene, &camera);
    }
    eprintln!("gpu frame {:?}", start.elapsed() / 10);
    assert!(compare("showcase", &cpu, &gpu) < 6.0);

    let focused = camera.focus_on(vec3(-0.8, 0.6, 0.6), 14.0);
    let cpu = cpu_renderer.render_pixels(&scene, &focused);
    let gpu = gpu_renderer.render_pixels(&scene, &focused);
    assert!(compare("showcase_dof", &cpu, &gpu) < 8.0);
}
