//! Compares the 3D of the film on the GPU with the CPU rasterizer.
use arm_example::Show;

#[test]
fn gpu_frames_match_cpu() {
    if !fframes_3d::Renderer::new(1, 1).uses_gpu() {
        eprintln!("no GPU, skipped");
        return;
    }
    let gpu = Show::new();
    let cpu = Show::new().on_cpu();
    let dir = std::env::temp_dir().join("arm-gpu");
    std::fs::create_dir_all(&dir).unwrap();
    for t in [0.6f32, 4.3, 8.0, 15.0, 17.6, 23.2, 34.1] {
        let start = std::time::Instant::now();
        let g = gpu.pixels(t);
        let gpu_time = start.elapsed();
        let start = std::time::Instant::now();
        let c = cpu.pixels(t);
        let cpu_time = start.elapsed();
        let diff: f64 = c
            .rgba
            .iter()
            .zip(&g.rgba)
            .map(|(a, b)| f64::from((i16::from(*a) - i16::from(*b)).unsigned_abs()))
            .sum::<f64>()
            / c.rgba.len() as f64;
        eprintln!(
            "{t:>5}s  gpu {gpu_time:>10.2?}  cpu {cpu_time:>10.2?}  mean difference {diff:.2}"
        );
        for (name, p) in [("gpu", &g), ("cpu", &c)] {
            image::save_buffer(
                dir.join(format!("{t}_{name}.png")),
                &p.to_straight_rgba(),
                p.width,
                p.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        assert!(diff < 3.0, "{t}: {diff}");
    }
}
