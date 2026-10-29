//! A moving shadow box must not change the frame: shadow maps stay still in the world, so
//! shadow edges do not crawl when the box follows the camera.
#![allow(clippy::unreadable_literal)]

use fframes_three_d::*;

#[allow(dead_code)]
mod common;

/// The mean brightness of the blocks of 8 x 8 pixels, so that the rounding to 8 bits
/// averages out.
fn blocks(p: &Pixels) -> Vec<f64> {
    let (w, h) = (p.width as usize / 8, p.height as usize / 8);
    let mut out = vec![0.0; w * h];
    for y in 0..h * 8 {
        for x in 0..w * 8 {
            let i = (y * p.width as usize + x) * 4;
            let sum: f64 = p.rgba[i..i + 3].iter().map(|&v| f64::from(v)).sum();
            out[(y / 8) * w + x / 8] += sum / (3.0 * 64.0);
        }
    }
    out
}

/// The mean of `|a - b|`.
fn difference(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| (a - b).abs()).sum::<f64>() / a.len() as f64
}

/// A box on a floor in the sun, with the shadow bounds around the point `center`.
fn shadowed_box(center: Vec3) -> (Scene, Camera) {
    let mut scene = Scene::new().background(Background::Color(Rgb::hex(0x202020)));
    let grey = Material::standard(0x909090).roughness(0.8);
    scene.add(Node::mesh(Geometry::floor(20.0, 20.0), grey.clone()));
    scene.add(Node::mesh(Geometry::cuboid(1.0, 1.0, 1.0), grey).at(0.3, 0.5, 0.0));
    scene.add(Node::light(
        Light::directional(Rgb::WHITE, 3.0, vec3(-3.0, 2.5, 1.0)).with_shadow(Shadow {
            map_size: 256,
            bounds: Some(Aabb {
                min: center - vec3(6.0, 1.0, 6.0),
                max: center + vec3(6.0, 2.0, 6.0),
            }),
            ..Default::default()
        }),
    ));
    let camera = Camera::perspective(40.0)
        .at(3.0, 4.0, 5.0)
        .look_at(vec3(0.3, 0.0, 0.0));
    (scene, camera)
}

/// Moves the shadow box by a fraction of a texel, as a box that follows the camera does.
fn shadow_change(renderer: &Renderer) -> f64 {
    // the map is 256 texels over about 16 m: one texel is about 6 cm
    let (scene, camera) = shadowed_box(vec3(0.0, 0.0, 0.0));
    let a = renderer.render_pixels(&scene, &camera);
    let (scene, camera) = shadowed_box(vec3(0.013, 0.004, -0.009));
    let b = renderer.render_pixels(&scene, &camera);
    common::save("temporal_shadow", &a);
    // the shadow is in the frame
    let mut unshadowed = scene;
    for (id, _) in unshadowed.clone().nodes() {
        if let Object::Light(Light::Directional(light)) = &mut unshadowed[id].object {
            light.shadow = None;
        }
    }
    let unshadowed = renderer.render_pixels(&unshadowed, &camera);
    let shadow = difference(&blocks(&b), &blocks(&unshadowed));
    assert!(shadow > 1.0, "{shadow}");
    a.rgba
        .iter()
        .zip(&b.rgba)
        .map(|(a, b)| f64::from(a.abs_diff(*b)))
        .sum::<f64>()
        / a.rgba.len() as f64
}

#[test]
fn shadow_map_stays_still_when_its_box_moves() {
    let change = shadow_change(&Renderer::new(320, 180).cpu());
    eprintln!("cpu: a box moved by a fraction of a texel changes the frame by {change:.4} / 255");
    assert!(change == 0.0, "{change}");
}

#[cfg(feature = "gpu")]
#[test]
fn gpu_shadow_map_stays_still_when_its_box_moves() {
    let renderer = Renderer::new(320, 180);
    if !renderer.uses_gpu() {
        eprintln!("no GPU, skipped");
        return;
    }
    let change = shadow_change(&renderer);
    eprintln!("gpu: a box moved by a fraction of a texel changes the frame by {change:.4} / 255");
    assert!(change == 0.0, "{change}");
}
