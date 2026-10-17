#![allow(clippy::unreadable_literal)]

use fframes_3d::*;

mod common;
use common::{cubes, save, showcase, studs};

#[test]
fn renders_showcase() {
    let (scene, camera) = showcase();
    let start = std::time::Instant::now();
    let pixels = Renderer::new(1920, 1080)
        .bloom(Bloom::default())
        .render_pixels(&scene, &camera);
    eprintln!("showcase 1080p: {:?}", start.elapsed());
    save("showcase", &pixels);
    // The environment background is opaque.
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

#[test]
fn instances_render_like_separate_meshes() {
    let renderer = Renderer::new(480, 270).cpu();
    let (instanced, camera) = cubes(true);
    let (separate, _) = cubes(false);
    let a = renderer.render_pixels(&instanced, &camera);
    let b = renderer.render_pixels(&separate, &camera);
    save("instances", &a);
    let diff = a
        .rgba
        .iter()
        .zip(&b.rgba)
        .map(|(x, y)| f64::from(x.abs_diff(*y)))
        .sum::<f64>()
        / a.rgba.len() as f64;
    eprintln!("instances against separate meshes: mean difference {diff:.3} / 255");
    assert!(diff < 0.1, "{diff}");
}

#[test]
fn renders_normal_maps() {
    let renderer = Renderer::new(960, 540).cpu();
    let (scene, camera) = studs();
    let bumpy = renderer.render_pixels(&scene, &camera);
    save("normal_map", &bumpy);
    let mut flat = scene.clone();
    for (id, _) in scene.nodes() {
        if let Object::Mesh(mesh) = &mut flat[id].object {
            mesh.material.normal_map = None;
        }
    }
    let flat = renderer.render_pixels(&flat, &camera);
    let diff = bumpy
        .rgba
        .iter()
        .zip(&flat.rgba)
        .map(|(a, b)| f64::from(a.abs_diff(*b)))
        .sum::<f64>()
        / bumpy.rgba.len() as f64;
    eprintln!("normal map changes the frame by {diff:.2} / 255 on average");
    assert!(diff > 2.0);
}

#[test]
fn textures_from_fframes_images() {
    use fframes::media::{ImageData, ImageMetadata};
    use fframes::usvgr::PreloadedImageData;

    // a red texel at half coverage next to an opaque blue one
    let data = PreloadedImageData::new("pair".into(), 2, 1, &[255, 0, 0, 128, 0, 0, 255, 255]);
    let image = ImageData::new_from_raw_data(
        std::sync::Arc::new(data),
        "pair".into(),
        ImageMetadata {
            width: 2,
            height: 1,
        },
    );
    let texture = Texture::from_image(&image);
    let red = texture.sample(vec2(0.25, 0.5));
    let blue = texture.sample(vec2(0.75, 0.5));
    assert!(red.x > 0.95 && red.y < 0.05, "{red}");
    assert!((red.w - 0.5).abs() < 0.01, "{red}");
    assert!(blue.z > 0.95 && blue.x < 0.05, "{blue}");
}

/// A blended particle covers only its opacity: a faint dark puff must not draw as a dark
/// disc. Both renderers (the GPU one when the `gpu` feature finds a GPU).
#[test]
fn faint_particles_stay_faint() {
    let mut scene = Scene::new().background(Background::Color(Rgb::hex(0x808080)));
    scene.add(Node::particles(
        vec![Particle::new(Vec3::ZERO, 1.0, Rgb::hex(0x000000)).opacity(0.1)],
        Blending::Normal,
    ));
    let camera = Camera::perspective(40.0)
        .at(0.0, 0.0, 6.0)
        .look_at(Vec3::ZERO);
    let renderer = Renderer::new(200, 200);
    for renderer in [renderer.clone(), renderer.cpu()] {
        let pixels = renderer.render_pixels(&scene, &camera);
        let (center, corner) = (pixels.get(100, 100)[0], pixels.get(5, 5)[0]);
        assert!(
            f32::from(center) > 0.8 * f32::from(corner),
            "center {center}, background {corner}"
        );
    }
}

/// An opaque material covers its pixels whatever the alpha of its texture, as in three.js.
/// Both renderers (the GPU one when the `gpu` feature finds a GPU).
#[test]
fn opaque_surfaces_ignore_texture_alpha() {
    let clear = Texture::from_rgba8(1, 1, &[255, 255, 255, 0], true);
    let mut scene = Scene::new();
    scene.add(Node::mesh(
        Geometry::plane(4.0, 4.0),
        Material::unlit(0x808080).map(clear),
    ));
    let camera = Camera::perspective(40.0).at(0.0, 0.0, 3.0);
    let renderer = Renderer::new(64, 64);
    for renderer in [renderer.clone(), renderer.cpu()] {
        let pixels = renderer.render_pixels(&scene, &camera);
        assert_eq!(pixels.get(32, 32)[3], 255, "gpu: {}", renderer.uses_gpu());
    }
}

/// A camera at its target (an orbit of radius 0) still gives a picture, not NaN.
#[test]
fn camera_at_its_target_renders() {
    let mut scene = Scene::new().background(Background::Color(Rgb::hex(0x336699)));
    scene.add(
        Node::mesh(Geometry::cuboid(1.0, 1.0, 1.0), Material::unlit(0xffffff)).at(0.0, 0.0, -3.0),
    );
    let camera = Camera::perspective(40.0).orbit(Vec3::ZERO, 0.0, 0.0, 0.0);
    assert!(camera.view_matrix().is_finite());
    let pixels = Renderer::new(64, 64).cpu().render_pixels(&scene, &camera);
    // it looks down -Z, at the white box in front of the blue background
    let (center, corner) = (pixels.get(32, 32), pixels.get(1, 1));
    assert!(center[0] > 200 && center[0] == center[2], "{center:?}");
    assert!(corner[2] > corner[0], "{corner:?}");
}
