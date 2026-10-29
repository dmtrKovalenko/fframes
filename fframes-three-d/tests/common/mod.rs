//! The scene and the image output that the render tests share.
#![allow(clippy::unreadable_literal)]
use std::path::PathBuf;

use fframes_three_d::*;

/// Writes `name.png` to `$FFRAMES_3D_OUT`, or to `fframes_three_d` in the temp directory.
pub fn save(name: &str, pixels: &Pixels) {
    let dir = std::env::var_os("FFRAMES_3D_OUT").map_or_else(
        || std::env::temp_dir().join("fframes_three_d"),
        PathBuf::from,
    );
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

/// A reflective floor, objects with different geometries and materials, sparks and two
/// lights, in front of the blurred environment.
pub fn showcase() -> (Scene, Camera) {
    let mut scene = Scene::new()
        .environment(Environment::studio())
        .background(Background::Environment { blur: 0.5 })
        .reflector(Reflector::floor());
    scene.add(
        Node::mesh(
            Geometry::floor(30.0, 30.0),
            Material::standard(0x202228).roughness(0.6).reflection(0.7),
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
    // a shell of sparks on a golden angle spiral
    let sparks: Vec<Particle> = (0..80)
        .map(|i| {
            let y = 1.0 - (i as f32 + 0.5) / 40.0;
            let (sin, cos) = (i as f32 * 2.399_963).sin_cos();
            let d = vec3(cos, 0.0, sin) * (1.0 - y * y).sqrt() + Vec3::Y * y;
            let distance = 0.4 + (i % 9) as f32 * 0.1;
            Particle::new(
                vec3(2.9, 0.9, -0.8) + d * distance,
                0.015,
                Rgb::hex(0xffaa33) * 10.0,
            )
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

/// A grid of cubes as one instanced mesh, or as separate meshes with the same transforms
/// and colors, lit by the sun, on a floor.
pub fn cubes(instanced: bool) -> (Scene, Camera) {
    let mut scene = Scene::new()
        .environment(Environment::studio())
        .background(Background::Color(Rgb::hex(0x101216)));
    scene.add(Node::mesh(
        Geometry::floor(20.0, 20.0),
        Material::standard(0x707478),
    ));
    scene.add(Node::light(Light::sun(
        Rgb::WHITE,
        2.5,
        vec3(3.0, 6.0, 4.0),
    )));
    let cube = std::sync::Arc::new(Geometry::rounded_box(0.5, 0.5, 0.5, 0.06, 3));
    let material = Material::standard(0xffffff).roughness(0.35);
    let copies: Vec<(Mat4, Rgb)> = (0..49)
        .map(|i| {
            let (x, z) = ((i % 7) as f32 - 3.0, (i / 7) as f32 - 3.0);
            let turn = Quat::from_rotation_y(i as f32 * 0.37);
            let place = Mat4::from_rotation_translation(turn, vec3(x * 0.8, 0.3, z * 0.8));
            let color = Rgb::linear(0.2 + 0.1 * (i % 5) as f32, 0.4, 0.9 - 0.1 * (i % 7) as f32);
            (place, color)
        })
        .collect();
    if instanced {
        let instances =
            Instances::new(copies.iter().map(|(m, _)| *m)).colors(copies.iter().map(|(_, c)| *c));
        scene.add(Node::instanced(cube, material, instances));
    } else {
        for (m, color) in copies {
            let (scale, rotation, position) = m.to_scale_rotation_translation();
            let mut node = Node::mesh(cube.clone(), material.clone().color(color));
            node.position = position;
            node.rotation = rotation;
            node.scale = scale;
            scene.add(node);
        }
    }
    let camera = Camera::perspective(40.0)
        .at(4.5, 4.0, 6.0)
        .look_at(vec3(0.0, 0.2, 0.0));
    (scene, camera)
}

/// A floor and a sphere with a normal map of round studs and an ambient occlusion map that
/// repeats twice as often, lit at a low angle so that the studs cast highlights.
pub fn studs() -> (Scene, Camera) {
    // a dome on each of 8 x 8 cells: the normal points away from the cell center
    let dome = |u: f32, v: f32| {
        let (x, y) = ((u * 8.0).fract() - 0.5, (v * 8.0).fract() - 0.5);
        let r2 = (x * x + y * y) / 0.16;
        if r2 >= 1.0 {
            return (Vec3::Z, 1.0);
        }
        // up in the image is -v, so the green channel takes -y
        let n = vec3(x, -y, 0.0) * 2.5 + Vec3::Z * (1.0 - r2).sqrt();
        (n.normalize(), 1.0)
    };
    let normal = Texture::from_fn_data(256, 256, false, |u, v| {
        let n = dome(u, v).0 * 0.5 + 0.5;
        [
            (n.x * 255.0) as u8,
            (n.y * 255.0) as u8,
            (n.z * 255.0) as u8,
            255,
        ]
    });
    let ao = Texture::from_fn_data(256, 256, false, |u, v| {
        let (x, y) = ((u * 8.0).fract() - 0.5, (v * 8.0).fract() - 0.5);
        let ring = ((x * x + y * y).sqrt() - 0.42).abs();
        let a = (180.0 + ring.min(0.06) / 0.06 * 75.0) as u8;
        [a, a, a, 255]
    });
    let (normal, ao) = (
        std::sync::Arc::new(normal),
        std::sync::Arc::new(ao.repeat(2.0, 2.0)),
    );
    let material = Material::standard(0x8a6e52)
        .roughness(0.35)
        .normal_map(normal.clone())
        .ao_map(ao.clone());
    let mut scene = Scene::new()
        .environment(Environment::studio())
        .background(Background::Color(Rgb::hex(0x101216)));
    scene.add(Node::mesh(Geometry::floor(8.0, 8.0), material.clone()));
    scene
        .add(Node::mesh(Geometry::sphere(0.8, 64, 32), material.color(0x4a6a8a)).at(0.0, 0.8, 0.0));
    scene.add(Node::light(Light::sun(
        Rgb::WHITE,
        3.0,
        vec3(-4.0, 1.5, -1.0),
    )));
    let camera = Camera::perspective(40.0)
        .at(0.0, 2.2, 4.0)
        .look_at(vec3(0.0, 0.5, 0.0));
    (scene, camera)
}
