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

    let moment = |o: f32| (scene.clone(), camera.fov(camera.fov + o * 4.0));
    let cpu = cpu_renderer.render_motion_blur_pixels(3, moment);
    let gpu = gpu_renderer.render_motion_blur_pixels(3, moment);
    assert!(compare("showcase_motion_blur", &cpu, &gpu) < 6.0);
}

#[test]
fn gpu_instances_match_cpu() {
    let gpu_renderer = Renderer::new(960, 540).bloom(Bloom::default());
    if !gpu_renderer.uses_gpu() {
        eprintln!("no GPU, skipped");
        return;
    }
    let (scene, camera) = common::cubes(true);
    let cpu = gpu_renderer.clone().cpu().render_pixels(&scene, &camera);
    let gpu = gpu_renderer.render_pixels(&scene, &camera);
    assert!(compare("instances", &cpu, &gpu) < 6.0);
}

#[test]
fn gpu_normal_maps_match_cpu() {
    let gpu_renderer = Renderer::new(960, 540);
    if !gpu_renderer.uses_gpu() {
        eprintln!("no GPU, skipped");
        return;
    }
    let (scene, camera) = common::studs();
    let cpu = gpu_renderer.clone().cpu().render_pixels(&scene, &camera);
    let gpu = gpu_renderer.render_pixels(&scene, &camera);
    assert!(compare("normal_map", &cpu, &gpu) < 4.0);
}

/// A miniature of a few millimeters at `origin`: a pebble with a rod over it on a plate,
/// lit by a sun with a shadow. The camera is 3 cm away, so a pixel is about 40
/// micrometers wide, as in a 4K close-up of a hand. The sun has a shadow map of `map_size`.
fn miniature(origin: Vec3, map_size: u32) -> (Scene, Camera) {
    let mut scene = Scene::new().environment(Environment::studio());
    let at = |p: Vec3| origin + p;
    scene.add(
        Node::mesh(
            Geometry::cuboid(0.03, 0.001, 0.03),
            Material::standard(0x8a8f99).roughness(0.7),
        )
        .with_position(at(vec3(0.0, -0.0005, 0.0)))
        .shadows(false, true),
    );
    scene.add(
        Node::mesh(
            Geometry::sphere(0.004, 20, 10),
            Material::standard(0xd08850).roughness(0.45),
        )
        .with_position(at(vec3(0.0, 0.004, 0.0))),
    );
    scene.add(
        Node::mesh(
            Geometry::capsule(0.0012, 0.012, 8, 16),
            Material::standard(0x5f8fd0).roughness(0.35),
        )
        .with_position(at(vec3(0.002, 0.011, 0.002)))
        .with_rotation(Quat::from_rotation_z(1.3)),
    );
    scene.add(Node::light(
        Light::directional(Rgb::WHITE, 3.0, vec3(-0.4, 1.0, 0.3)).with_shadow(Shadow {
            map_size,
            ..Shadow::default()
        }),
    ));
    scene.add(Node::light(Light::hemisphere(0x9fb4d8, 0x30281f, 0.6)));
    let camera = Camera::perspective(30.0)
        .clip(0.002, 1.0)
        .position(at(vec3(0.016, 0.016, 0.022)))
        .look_at(at(vec3(0.0, 0.004, 0.0)));
    (scene, camera)
}

/// In a close view the steps of the surface between two pixels are micrometers. The GPU
/// must find the normal of the triangle from them, also far from the origin. A normal lost
/// to the precision of 32 bit floats shows as dots along the triangle edges and as patches
/// of wrong shadow.
#[test]
fn gpu_close_views_match_cpu() {
    let gpu_renderer = Renderer::new(480, 320);
    if !gpu_renderer.uses_gpu() {
        eprintln!("no GPU, skipped");
        return;
    }
    for (name, origin) in [
        ("miniature", Vec3::ZERO),
        ("miniature_far", vec3(70.0, 3.0, -45.0)),
    ] {
        let (scene, camera) = miniature(origin, 2048);
        let cpu = gpu_renderer.clone().cpu().render_pixels(&scene, &camera);
        let gpu = gpu_renderer.render_pixels(&scene, &camera);
        compare(name, &cpu, &gpu);
        // The wrong normals cover a few percent of the picture with strong errors.
        let wrong = (cpu.rgba.as_chunks::<4>().0.iter())
            .zip(gpu.rgba.as_chunks::<4>().0)
            .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 32))
            .count();
        let share = wrong as f32 / (cpu.width * cpu.height) as f32;
        eprintln!("{name}: {:.3} % of the pixels differ", share * 100.0);
        assert!(share < 0.005, "{name}");
    }
}

/// A frame must not depend on the frames before it. The GPU keeps its frame contexts (the
/// targets, the shadow maps, the buffers) between frames, and an earlier shot with larger
/// shadow maps left them larger: the next shot was shaded with a finer shadow map than on
/// the CPU, with acne along the triangles of a close-up.
#[test]
fn gpu_frames_do_not_depend_on_earlier_frames() {
    // A size of its own, so that the other tests do not share the frame contexts.
    let renderer = Renderer::new(410, 290);
    if !renderer.uses_gpu() {
        eprintln!("no GPU, skipped");
        return;
    }
    let (scene, camera) = miniature(Vec3::ZERO, 2048);
    let fresh = renderer.render_pixels(&scene, &camera);
    // Another shot with a larger shadow map and motion blur.
    let (other, other_camera) = miniature(vec3(1.0, 0.0, 0.0), 8192);
    let _ = renderer.render_motion_blur_pixels(3, |o| (other.clone(), other_camera.fov(30.0 + o)));
    let after = renderer.render_pixels(&scene, &camera);
    compare("after_other_shot", &fresh, &after);
    assert_eq!(fresh.rgba, after.rgba);
}

/// A shadow light with small bounds far from the origin, and a caster far above them,
/// beyond the near plane of the shadow camera. The caster still throws its shadow.
#[test]
fn gpu_casters_beyond_the_shadow_bounds_cast() {
    let renderer = Renderer::new(320, 240);
    if !renderer.uses_gpu() {
        eprintln!("no GPU, skipped");
        return;
    }
    let origin = vec3(70.0, 2.0, -12.0);
    let mut scene = Scene::new();
    scene.add(
        Node::mesh(
            Geometry::cuboid(0.7, 0.02, 0.7),
            Material::standard(0xb0b0b0).roughness(0.8),
        )
        .with_position(origin - Vec3::Y * 0.01),
    );
    // 5 m over the plate. The shadow camera starts about 1 m over it.
    scene.add(
        Node::mesh(
            Geometry::cuboid(0.3, 0.05, 0.3),
            Material::standard(0x808080),
        )
        .with_position(origin + Vec3::Y * 5.0),
    );
    scene.add(Node::light(
        Light::directional(Rgb::WHITE, 3.0, Vec3::Y).with_shadow(Shadow {
            bounds: Some(Aabb {
                min: origin - Vec3::splat(0.35),
                max: origin + Vec3::splat(0.35),
            }),
            ..Shadow::default()
        }),
    ));
    scene.add(Node::light(Light::ambient(Rgb::WHITE, 0.3)));
    let camera = Camera::perspective(40.0)
        .position(origin + vec3(0.0, 1.2, 0.9))
        .look_at(origin);
    let cpu = renderer.clone().cpu().render_pixels(&scene, &camera);
    let gpu = renderer.render_pixels(&scene, &camera);
    assert!(compare("far_caster", &cpu, &gpu) < 1.0);
    let center = camera.project(origin, 320.0, 240.0).unwrap();
    let corner = camera
        .project(origin + vec3(0.3, 0.0, 0.3), 320.0, 240.0)
        .unwrap();
    for (name, pixels) in [("cpu", &cpu), ("gpu", &gpu)] {
        let luma = |p: Vec3| {
            let i = ((p.y as u32 * pixels.width + p.x as u32) * 4) as usize;
            pixels.rgba[i..i + 3]
                .iter()
                .map(|&c| f32::from(c))
                .sum::<f32>()
        };
        let (shaded, lit) = (luma(center), luma(corner));
        eprintln!("{name}: under the caster {shaded}, in the light {lit}");
        assert!(shaded < lit * 0.6, "{name}: {shaded} {lit}");
    }
}
