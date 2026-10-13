#![allow(clippy::unreadable_literal)]

use fframes_3d::*;

/// Renders three poses of `RobotExpressive.glb` from three.js (CC0, Tomás Laulhé), a model
/// with a skin, morph targets and 14 clips. It runs when `FFRAMES_3D_ROBOT` is the path of
/// a copy.
#[test]
fn renders_skinned_gltf() {
    let Ok(path) = std::env::var("FFRAMES_3D_ROBOT") else {
        return;
    };
    let model = Model::from_gltf(&std::fs::read(path).unwrap()).unwrap();
    let names: Vec<&str> = model.animations.iter().map(|a| a.name.as_str()).collect();
    eprintln!("clips: {names:?}, bounds {:?}", model.bounds());
    let dance = model.animation("Dance").unwrap();

    let mut stage = Scene::new()
        .environment(Environment::studio())
        .background(Background::Color(Rgb::hex(0x202020)));
    stage.add(
        Node::mesh(Geometry::floor(40.0, 40.0), Material::standard(0x404040)).shadows(false, true),
    );
    stage.add(Node::light(Light::hemisphere(0xffffff, 0x444444, 2.0)));
    stage.add(Node::light(Light::sun(
        Rgb::WHITE,
        2.5,
        vec3(0.0, 20.0, 10.0),
    )));
    let camera = Camera::perspective(45.0)
        .at(-5.0, 3.0, 10.0)
        .look_at(vec3(0.0, 2.0, 0.0));

    let dir = std::env::temp_dir().join("fframes-3d");
    std::fs::create_dir_all(&dir).unwrap();
    for (i, t) in [0.0, 0.4, 0.8].into_iter().enumerate() {
        let mut scene = stage.clone();
        let robot = model.add_to(&mut scene, None);
        robot.play(&mut scene, dance, t);
        let pixels = Renderer::new(960, 540).render_pixels(&scene, &camera);
        image::save_buffer(
            dir.join(format!("robot_{i}.png")),
            &pixels.to_straight_rgba(),
            pixels.width,
            pixels.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
