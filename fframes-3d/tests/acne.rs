#![allow(clippy::unreadable_literal)]

use fframes_3d::*;

/// A capsule and a sphere above a shadow catcher, lit by a sun with the given shadow.
fn scene(shadow: Option<Shadow>) -> Scene {
    let mut s = Scene::new().environment(Environment::studio());
    s.add(
        Node::mesh(Geometry::floor(40.0, 40.0), Material::shadow_catcher(0.32))
            .shadows(false, true),
    );
    s.add(
        Node::mesh(
            Geometry::capsule(0.07, 0.26, 8, 40),
            Material::standard(0xf2f2ef).roughness(0.55),
        )
        .at(0.0, 1.0, 0.0),
    );
    s.add(
        Node::mesh(
            Geometry::sphere(0.095, 32, 16),
            Material::metal(0xe2e5e9, 0.14),
        )
        .at(0.0, 1.2, 0.0),
    );
    let mut sun = Light::directional(Rgb::WHITE, 2.6, vec3(2.5, 9.0, 4.0));
    if let Some(shadow) = shadow {
        sun = sun.with_shadow(shadow);
    }
    s.add(Node::light(sun));
    s.add(Node::light(Light::hemisphere(0xffffff, 0x999999, 2.0)));
    s
}

#[test]
#[ignore = "writes images for looking at contact shadows"]
fn contact_shadows() {
    let camera = Camera::perspective(20.0)
        .at(0.0, 1.5, 3.2)
        .look_at(vec3(0.0, 1.1, 0.0));
    let dir = std::env::temp_dir().join("fframes-3d");
    std::fs::create_dir_all(&dir).unwrap();
    let shadow = |softness| {
        Some(Shadow {
            softness,
            ..Default::default()
        })
    };
    for (name, shadow) in [("off", None), ("hard", shadow(0.0)), ("soft", shadow(5.0))] {
        let p = Renderer::new(400, 400).render_pixels(&scene(shadow), &camera);
        image::save_buffer(
            dir.join(format!("acne_{name}.png")),
            &p.to_straight_rgba(),
            p.width,
            p.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
