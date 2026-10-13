//! The night road: wet asphalt, lane markings, neon pylons, a skyline in the fog, and the
//! lights.

use fframes_3d::math::hash;
use fframes_3d::*;

pub const CYAN: u32 = 0x22e8ff;
pub const MAGENTA: u32 = 0xff2bd6;

/// The stage lights that the effects animate.
pub struct Stage {
    pub headlight: NodeId,
    pub rim_cyan: NodeId,
    pub rim_magenta: NodeId,
}

/// A dark studio with neon strips for the reflections.
fn environment() -> Environment {
    Environment::Studio(Studio {
        sky: Rgb::hex(0x0a0d1c),
        horizon: Rgb::hex(0x2a1838),
        ground: Rgb::hex(0x040406),
        softboxes: vec![
            // A cool key light from above.
            SoftBox::new(vec3(-0.3, 1.0, 0.4), 60.0, 34.0, Rgb::hex(0xbcd0ff) * 2.2),
            // Neon strips on the left and on the right.
            SoftBox::new(vec3(1.0, 0.25, 0.1), 10.0, 80.0, Rgb::hex(CYAN) * 3.0),
            SoftBox::new(vec3(-1.0, 0.25, -0.2), 10.0, 80.0, Rgb::hex(MAGENTA) * 3.0),
            // A long, warm strip of street lights behind.
            SoftBox::new(vec3(0.0, 0.12, -1.0), 140.0, 4.0, Rgb::hex(0xffb070) * 1.2),
        ],
        intensity: 1.5,
        rotation: 0.0,
    })
}

/// Adds the road, the pylons, the skyline and the lights.
pub fn build(scene: &mut Scene) -> Stage {
    scene.environment = environment();
    scene.background = Background::Gradient {
        top: Rgb::hex(0x03040a),
        bottom: Rgb::hex(0x1c1030),
    };
    scene.fog = Some(Fog::Exp2 {
        color: Rgb::hex(0x120c22),
        density: 0.018,
    });

    // Wet asphalt.
    scene.add(
        Node::mesh(
            Geometry::floor(160.0, 600.0),
            Material::standard(0x0b0c10)
                .roughness(0.3)
                .env_intensity(0.35),
        )
        .shadows(false, true),
    );

    // Lane markings: dashes in the middle and solid lines at the edges.
    let mut dashes = Vec::new();
    let mut z = -300.0;
    while z < 300.0 {
        dashes.push(Geometry::floor(0.16, 3.0).translate(0.0, 0.01, z));
        z += 9.0;
    }
    scene.add(
        Node::mesh(
            Geometry::merge(dashes),
            Material::unlit(Rgb::hex(0xffcf6a) * 1.4),
        )
        .shadows(false, false),
    );
    let edges = Geometry::merge([
        Geometry::floor(0.14, 600.0).translate(5.2, 0.01, 0.0),
        Geometry::floor(0.14, 600.0).translate(-5.2, 0.01, 0.0),
    ]);
    scene.add(Node::mesh(edges, Material::unlit(Rgb::hex(0xdfe8ff) * 1.1)).shadows(false, false));

    // Neon pylons along both sides. The wet road shows each neon bar as a soft streak.
    let streak = |x: f32, z: f32| {
        let mut g = Geometry::floor_segments(0.5, 7.0, 1, 8);
        g.positions.iter_mut().for_each(|p| *p += vec3(x, 0.015, z));
        g.update_bounds();
        g.colors = g
            .positions
            .iter()
            .map(|p| Vec4::new(1.0, 1.0, 1.0, 1.0 - ((p.z - z).abs() / 3.5).min(1.0)))
            .collect();
        g
    };
    let mut cyan_streaks = Vec::new();
    let mut magenta_streaks = Vec::new();
    let mut cyan = Vec::new();
    let mut magenta = Vec::new();
    let mut posts = Vec::new();
    for i in -25..25 {
        let z = i as f32 * 12.0;
        for side in [-1.0f32, 1.0] {
            let x = side * 8.5;
            let bar = Geometry::rounded_box(0.14, 4.6, 0.14, 0.05, 2).translate(x, 2.6, z);
            if (i + i32::from(side <= 0.0)) % 2 == 0 {
                cyan.push(bar);
                cyan_streaks.push(streak(x, z));
            } else {
                magenta.push(bar);
                magenta_streaks.push(streak(x, z));
            }
            posts.push(Geometry::rounded_box(0.5, 0.3, 0.5, 0.06, 2).translate(x, 0.15, z));
        }
    }
    let neon = |color: u32| Material::standard(0x050505).emissive(color, 5.0);
    scene.add(Node::mesh(Geometry::merge(cyan), neon(CYAN)).shadows(false, false));
    scene.add(Node::mesh(Geometry::merge(magenta), neon(MAGENTA)).shadows(false, false));
    scene.add(
        Node::mesh(Geometry::merge(posts), Material::metal(0x24262c, 0.4)).shadows(false, false),
    );
    scene.add(
        Node::mesh(Geometry::merge(cyan_streaks), Material::glow(CYAN, 0.5)).shadows(false, false),
    );
    scene.add(
        Node::mesh(
            Geometry::merge(magenta_streaks),
            Material::glow(MAGENTA, 0.5),
        )
        .shadows(false, false),
    );

    // The skyline: dark towers with lit windows. They are far away, so the fog makes them soft.
    let windows = std::sync::Arc::new(Texture::from_fn(128, 256, |u, v| {
        let cx = (u * 16.0) as u32;
        let cy = (v * 48.0) as u32;
        let inside = (u * 16.0).fract() > 0.3 && (v * 48.0).fract() > 0.4;
        let lit = hash(cx * 131 + cy * 7) > 0.7;
        if inside && lit {
            let warm = hash(cx + cy * 17) > 0.5;
            if warm {
                fframes::Color::rgb(255, 196, 130)
            } else {
                fframes::Color::rgb(160, 210, 255)
            }
        } else {
            fframes::Color::rgb(0, 0, 0)
        }
    }));
    let mut towers = Vec::new();
    for i in 0..44 {
        let h = hash(i * 3 + 1);
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let x = side * (40.0 + hash(i * 5 + 2) * 50.0);
        let z = -200.0 + i as f32 * 10.0 + hash(i * 7) * 6.0;
        let width = 12.0 + h * 14.0;
        let height = 30.0 + hash(i * 11) * 70.0;
        towers.push(Geometry::cuboid(width, height, width).translate(x, height * 0.5, z));
    }
    let mut tower_material = Material::standard(0x07080c)
        .roughness(0.6)
        .emissive(Rgb::WHITE, 1.0);
    tower_material.emissive_map = Some(windows);
    scene.add(Node::mesh(Geometry::merge(towers), tower_material).shadows(false, false));

    scene.add(Node::light(
        Light::sun(Rgb::hex(0xc8d8ff), 3.2, vec3(-5.0, 12.0, 7.0)).with_shadow(Shadow {
            map_size: 2048,
            softness: 2.0,
            strength: 0.92,
            ..Default::default()
        }),
    ));
    scene.add(Node::light(Light::hemisphere(0x4a5ca8, 0x1a0c20, 2.2)));
    let rim_cyan = scene.add(Node::light(Light::point(CYAN, 0.0, 0.0)).at(5.0, 3.5, -2.0));
    let rim_magenta = scene.add(Node::light(Light::point(MAGENTA, 0.0, 0.0)).at(-5.0, 3.0, -1.5));
    let headlight = scene.add(Node::light(
        Light::spot(0xe8f2ff, 0.0, deg(28.0), 0.5).pointing(vec3(0.0, -0.18, 1.0)),
    ));

    Stage {
        headlight,
        rim_cyan,
        rim_magenta,
    }
}
