//! Measures where the GPU frame time goes:
//! `cargo test --release -p arm-example --test gpu_bench -- --ignored --nocapture`
use arm_example::Show;
use fframes_3d::Renderer;

#[test]
#[ignore = "frame time breakdown, run with --nocapture"]
fn gpu_frame_breakdown() {
    let show = Show::new();
    let (scene, camera) = show.scene_at(8.0);
    let base = Renderer::new(1920, 1080)
        .gpu()
        .bloom(fframes_3d::Bloom::default());
    let time = |name: &str, renderer: &Renderer, camera: &fframes_3d::Camera| {
        renderer.render_pixels(&scene, camera);
        let start = std::time::Instant::now();
        for _ in 0..20 {
            renderer.render_pixels(&scene, camera);
        }
        eprintln!("{name:<28} {:?}", start.elapsed() / 20);
    };
    let mut no_focus = camera;
    no_focus.focus = None;
    let mut no_bloom = base.clone();
    no_bloom.bloom = None;
    time("everything", &base, &camera);
    time("no depth of field", &base, &no_focus);
    time("no dof, no bloom", &no_bloom, &no_focus);
    let start = std::time::Instant::now();
    for _ in 0..20 {
        let _ = show.scene_at(8.0);
    }
    eprintln!("{:<28} {:?}", "scene_at (cpu posing)", start.elapsed() / 20);
}
