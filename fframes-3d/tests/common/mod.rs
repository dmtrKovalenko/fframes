//! The scene and the image output that the render tests share.
#![allow(clippy::unreadable_literal)]
use std::path::PathBuf;

use fframes_3d::*;

/// Writes `name.png` to `$FFRAMES_3D_OUT`, or to `fframes-3d` in the temp directory.
pub fn save(name: &str, pixels: &Pixels) {
    let dir = std::env::var_os("FFRAMES_3D_OUT")
        .map_or_else(|| std::env::temp_dir().join("fframes-3d"), PathBuf::from);
    std::fs::create_dir_all(&dir).unwrap();
    image::save_buffer(
        dir.join(format!("{name}.png")),
        &pixels.to_straight_rgba(),
        pixels.width,
        pixels.height,
        image::ColorType::Rgba8,
    )
    .unwrap();
}

/// A reflective floor with a grid, objects with different geometries and materials, sparks
/// and two lights.
pub fn showcase() -> (Scene, Camera) {
    let mut scene = Scene::new()
        .environment(Environment::studio())
        .background(Background::Gradient {
            top: Rgb::hex(0x1a1d24),
            bottom: Rgb::hex(0x07080a),
        })
        .reflector(Reflector::floor());
    scene.add(
        Node::mesh(
            Geometry::floor(30.0, 30.0),
            Material::standard(0x202228)
                .roughness(0.6)
                .reflection(0.7)
                .grid(1.0, 0.02, Rgb::hex(0x30a0ff) * 2.0, 12.0),
        )
        .shadows(false, true),
    );
    scene.add(
        Node::mesh(
            Geometry::rounded_box(1.0, 1.0, 1.0, 0.08, 4),
            Material::metal(0xd4d8e0, 0.25),
        )
        .at(-2.2, 0.5, 0.0)
        .rotate_y(0.5),
    );
    scene.add(
        Node::mesh(
            Geometry::sphere(0.6, 48, 24),
            Material::standard(0xff4d1a).roughness(0.35).clearcoat(1.0),
        )
        .at(-0.8, 0.6, 0.6),
    );
    scene.add(
        Node::mesh(
            Geometry::cylinder(0.4, 0.5, 1.2, 40),
            Material::metal(0xffb84d, 0.4),
        )
        .at(0.6, 0.6, -0.2),
    );
    scene.add(
        Node::mesh(
            Geometry::torus(0.45, 0.15, 24, 64),
            Material::standard(0x2a6dff).roughness(0.2),
        )
        .at(2.0, 0.6, 0.4)
        .rotate_x(-0.4),
    );
    scene.add(
        Node::mesh(
            Geometry::extrude(&Geometry::polygon(0.5, 6), 0.25, 0.05),
            Material::metal(0x9aa3ad, 0.5),
        )
        .at(0.0, 1.6, -1.2),
    );
    scene.add(
        Node::mesh(
            Geometry::icosahedron(0.35, 0),
            Material::standard(0x44dd88).flat(),
        )
        .at(1.4, 0.35, 1.3),
    );
    scene.add(
        Node::mesh(
            Geometry::capsule(0.18, 0.5, 6, 24),
            Material::standard(0x111111).emissive(0x33ddff, 6.0),
        )
        .at(2.9, 0.45, -0.8),
    );
    scene.add(
        Node::mesh(
            Geometry::sphere(0.5, 32, 16),
            Material::standard(0x99ccff).opacity(0.35),
        )
        .at(-1.4, 1.6, -1.0),
    );
    let sparks: Vec<Particle> = (0..80)
        .map(|i| {
            let d = math::random_direction(i);
            Particle::new(
                vec3(2.9, 0.9, -0.8) + d * (0.4 + math::hash(i * 7) * 0.8),
                0.015,
                Rgb::hex(0xffaa33) * 10.0,
            )
            .tail(-d * 0.25)
        })
        .collect();
    scene.add(Node::particles(sparks, Blending::Additive));
    scene.add(Node::light(Light::sun(
        Rgb::WHITE,
        2.5,
        vec3(3.0, 6.0, 4.0),
    )));
    scene.add(Node::light(Light::point(0x3399ff, 6.0, 0.0)).at(-3.0, 1.5, 2.0));
    let camera = Camera::perspective(40.0)
        .at(0.5, 3.0, 7.5)
        .look_at(vec3(0.3, 0.6, 0.0));
    (scene, camera)
}
