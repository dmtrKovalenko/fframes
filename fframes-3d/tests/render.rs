#![allow(clippy::unreadable_literal)]

use fframes_3d::*;

mod common;
use common::{save, showcase};

#[test]
fn renders_showcase() {
    let (scene, camera) = showcase();
    let start = std::time::Instant::now();
    let pixels = Renderer::new(1920, 1080)
        .bloom(Bloom::default())
        .render_pixels(&scene, &camera);
    eprintln!("showcase 1080p: {:?}", start.elapsed());
    save("showcase", &pixels);
    // The gradient background is opaque.
    assert_eq!(pixels.get(5, 5)[3], 255);
}

#[test]
fn renders_text_with_holes() {
    let Ok(font) = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/conference-splash-screen/media/Inter_18pt-ExtraBold.ttf"
    )) else {
        return;
    };
    let text = Geometry::text(&font, "fframes aeo8B", 1.0, 0.25, 0.03).unwrap();
    let b = text.bounds();
    let mut scene = Scene::new()
        .environment(Environment::studio())
        .background(Background::Color(Rgb::hex(0xf2f2f0)));
    scene.add(
        Node::mesh(
            text,
            Material::standard(0x141414).roughness(0.3).clearcoat(1.0),
        )
        .at(-b.center().x, 0.0, 0.0),
    );
    scene.add(Node::light(Light::sun(
        Rgb::WHITE,
        2.5,
        vec3(2.0, 5.0, 4.0),
    )));
    let camera = Camera::perspective(30.0)
        .at(0.0, 0.35, 5.5)
        .look_at(vec3(0.0, 0.35, 0.0));
    let pixels = Renderer::new(1600, 400).render_pixels(&scene, &camera);
    save("text", &pixels);
    assert!(b.size().x > 5.0);
}

#[test]
fn renders_depth_of_field() {
    let (scene, camera) = showcase();
    let camera = camera.focus_on(vec3(-0.8, 0.6, 0.6), 14.0);
    let start = std::time::Instant::now();
    let pixels = Renderer::new(1920, 1080).render_pixels(&scene, &camera);
    eprintln!("depth of field 1080p: {:?}", start.elapsed());
    save("depth_of_field", &pixels);
}

/// Renders an STL file into `stl.png` with the lighting of the showcase. Run it with
/// `FFRAMES_3D_STL=model.stl cargo test -p fframes-3d --test render renders_an_stl -- --nocapture`.
#[test]
fn renders_an_stl() {
    let Ok(path) = std::env::var("FFRAMES_3D_STL") else {
        return;
    };
    let bytes = std::fs::read(&path).unwrap();
    let geometry = Geometry::from_stl(&bytes).unwrap().smooth(40.0);
    // Fits the model into a 1.6 m box on the floor, for all units and up axes.
    let bounds = geometry.bounds();
    let size = bounds.size().max_element();
    let geometry = geometry
        .translate(-bounds.center().x, -bounds.min.y, -bounds.center().z)
        .scale(1.6 / size, 1.6 / size, 1.6 / size);
    eprintln!(
        "{path}: {} triangles, {} vertices after smoothing",
        geometry.triangle_count(),
        geometry.vertex_count()
    );
    let mut scene = Scene::new()
        .environment(Environment::studio())
        .reflector(Reflector::floor());
    scene.add(Node::mesh(
        Geometry::floor(20.0, 20.0),
        Material::standard(0x2a2c30).roughness(0.35).reflection(0.6),
    ));
    scene.add(Node::mesh(
        geometry,
        Material::standard(0xd8d2c4).roughness(0.4).clearcoat(0.6),
    ));
    scene.add(Node::light(Light::sun(
        Rgb::WHITE,
        2.5,
        vec3(2.0, 5.0, 3.0),
    )));
    let camera = Camera::perspective(32.0)
        .at(2.2, 1.6, 3.0)
        .look_at(vec3(0.0, 0.7, 0.0));
    let pixels = Renderer::new(960, 540)
        .bloom(Bloom::default())
        .render_pixels(&scene, &camera);
    save("stl", &pixels);
}
