//! The lab: a polished concrete floor, the robot cell with hazard stripes and stencils, the
//! letter rack, the walls and the lights.

use std::f32::consts::TAU;
use std::sync::Arc;

use fframes_3d::math::{hash, smoothstep};
use fframes_3d::*;

/// Value noise in `0..1` that repeats every `period` cells.
fn noise(x: f32, y: f32, period: u32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let p = period as i32;
    let at = |i: i32, j: i32| {
        hash(
            (i.rem_euclid(p) as u32).wrapping_mul(7919)
                ^ (j.rem_euclid(p) as u32).wrapping_mul(104_729)
                ^ seed.wrapping_mul(31),
        )
    };
    let (i, j) = (xi as i32, yi as i32);
    let (u, v) = (smoothstep(0.0, 1.0, fx), smoothstep(0.0, 1.0, fy));
    let a = at(i, j) + (at(i + 1, j) - at(i, j)) * u;
    let b = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * u;
    a + (b - a) * v
}

/// Five octaves of [`noise`].
fn fbm(u: f32, v: f32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut period = 8u32;
    for o in 0..5 {
        sum += amp * noise(u * period as f32, v * period as f32, period, seed + o);
        amp *= 0.5;
        period *= 2;
    }
    sum
}

/// Size of one floor texture tile in meters. One tile holds two by two concrete slabs.
const TILE: f32 = 2.4;

/// The color and roughness maps of the polished concrete: slab colors, mottling, specks,
/// seams and scuffs.
fn floor_textures() -> (Texture, Texture) {
    let size = 1024;
    let seam = |u: f32, v: f32| {
        let d = |c: f32| {
            let f = (c * 2.0).fract();
            f.min(1.0 - f) / 2.0 * TILE
        };
        let w = d(u).min(d(v));
        1.0 - ((w - 0.002) / 0.003).clamp(0.0, 1.0)
    };
    let color = Texture::from_fn(size, size, |u, v| {
        let slab = hash((u * 2.0) as u32 * 3 + (v * 2.0) as u32 * 7 + 11) * 0.05;
        let mottle = fbm(u, v, 3);
        let fine = noise(u * 256.0, v * 256.0, 256, 9);
        let speck = if hash((u * 900.0) as u32 * 977 + (v * 900.0) as u32) > 0.985 {
            0.06
        } else {
            0.0
        };
        let scuff = (fbm(u * 0.5 + 0.3, v * 2.0, 21) - 0.6).max(0.0) * 0.3;
        let mut l = 0.52 + slab + (mottle - 0.5) * 0.1 + (fine - 0.5) * 0.04 - speck - scuff;
        l *= 1.0 - 0.35 * seam(u, v);
        let g = (l * 255.0).clamp(0.0, 255.0) as u8;
        fframes::Color::rgb(
            g,
            (f32::from(g) * 1.005).min(255.0) as u8,
            (f32::from(g) * 1.02).min(255.0) as u8,
        )
    })
    .repeat(1.0 / TILE, 1.0 / TILE);
    let rough = Texture::from_fn_data(size / 2, size / 2, false, |u, v| {
        let wear = fbm(u + 0.17, v + 0.41, 13);
        let r = (0.32 + (wear - 0.5) * 0.35 + seam(u, v) * 0.4).clamp(0.12, 1.0);
        [255, (r * 255.0) as u8, 0, 255]
    })
    .repeat(1.0 / TILE, 1.0 / TILE);
    (color, rough)
}

/// Diagonal yellow and black stripes in world space.
fn hazard_texture() -> Texture {
    Texture::from_fn(64, 64, |u, v| {
        if (u + v).fract() < 0.5 {
            fframes::Color::rgb(236, 180, 0)
        } else {
            fframes::Color::rgb(22, 22, 24)
        }
    })
    .repeat(1.0 / 0.3, 1.0 / 0.3)
}

/// A strip of floor paint from `a` to `b`, with world space UVs.
fn strip(a: Vec2, b: Vec2, width: f32) -> Geometry {
    let len = a.distance(b);
    let dir = (b - a) / len;
    let angle = dir.y.atan2(dir.x);
    let mid = (a + b) * 0.5;
    let mut g = Geometry::floor(len, width)
        .rotate_y(-angle)
        .translate(mid.x, 0.003, mid.y);
    g.uvs = g.positions.iter().map(|p| Vec2::new(p.x, p.z)).collect();
    g
}

/// The lab nodes that [`set_lights`] changes.
pub struct Lab {
    pub key: NodeId,
    pub fill: NodeId,
    pub ambient: NodeId,
    pub light_strips: NodeId,
}

/// The studio environment of the lab at the light `level`.
fn environment(level: f32) -> Environment {
    Environment::Studio(Studio {
        sky: Rgb::hex(0xeef0f2),
        horizon: Rgb::hex(0xc9ccd0),
        ground: Rgb::hex(0x6a6c70),
        softboxes: vec![
            // Rows of light panels in the ceiling.
            SoftBox::new(vec3(-0.35, 1.0, 0.0), 12.0, 70.0, Rgb::WHITE * 3.2),
            SoftBox::new(vec3(0.35, 1.0, 0.0), 12.0, 70.0, Rgb::WHITE * 3.2),
            SoftBox::new(vec3(0.0, 0.5, 0.85), 70.0, 10.0, Rgb::hex(0xfff1e0) * 1.6),
            SoftBox::new(vec3(-1.0, 0.18, -0.3), 8.0, 40.0, Rgb::hex(0xdde8ff) * 1.4),
        ],
        intensity: 0.6 * level,
        rotation: 0.0,
    })
}

/// Adds the lab to `scene`.
pub fn build(scene: &mut Scene, font: &[u8]) -> Lab {
    let (color, rough) = floor_textures();
    // The scene reflects in the polished concrete. The roughness map blurs the reflection.
    scene.reflector = Some(Reflector::floor());
    let mut floor_material = Material::standard(0xffffff)
        .roughness(1.0)
        .env_intensity(0.9)
        .reflection(0.85);
    floor_material.map = Some(Arc::new(color));
    floor_material.metal_rough_map = Some(Arc::new(rough));
    scene.add(Node::mesh(Geometry::floor(60.0, 60.0), floor_material).shadows(false, true));

    // The robot cell: a hazard stripe border, a dashed circle for the reach, and stencils.
    let hazard = Material::standard(0xffffff)
        .roughness(0.55)
        .map(hazard_texture());
    let (x0, x1, z0, z1) = (-2.9f32, 2.9f32, -2.15f32, 2.35f32);
    let border = Geometry::merge([
        strip(vec2(x0, z0), vec2(x1, z0), 0.14),
        strip(vec2(x0, z1), vec2(x1, z1), 0.14),
        strip(vec2(x0, z0 - 0.07), vec2(x0, z1 + 0.07), 0.14),
        strip(vec2(x1, z0 - 0.07), vec2(x1, z1 + 0.07), 0.14),
    ]);
    scene.add(Node::mesh(border, hazard).shadows(false, true));
    let paint = Material::standard(0xf3f3f0).roughness(0.6);
    let dashes = Geometry::merge((0..48).filter(|i| i % 2 == 0).map(|i| {
        let a0 = i as f32 / 48.0 * TAU;
        let a1 = (i as f32 + 1.0) / 48.0 * TAU;
        strip(
            vec2(a0.sin(), a0.cos()) * 2.05,
            vec2(a1.sin(), a1.cos()) * 2.05,
            0.035,
        )
    }));
    scene.add(Node::mesh(dashes, paint.clone()).shadows(false, true));
    let ring = Geometry::ring(0.62, 0.66, 96)
        .rotate_x(deg(-90.0))
        .translate(0.0, 0.003, 0.0);
    scene.add(Node::mesh(ring, paint.clone()).shadows(false, true));
    let stencil = |text: &str, size: f32, at: Vec3, yaw: f32| -> Option<Geometry> {
        let g = Geometry::text(font, text, size, 0.002, 0.0).ok()?;
        let c = g.bounds().center();
        Some(
            g.translate(-c.x, -c.y, 0.0)
                .rotate_x(deg(-90.0))
                .rotate_y(yaw)
                .translate(at.x, 0.003, at.z),
        )
    };
    let mut stencils = Vec::new();
    stencils.extend(stencil("R-07", 0.22, vec3(1.15, 0.0, -0.75), deg(-30.0)));
    stencils.extend(stencil("OUTPUT", 0.13, vec3(1.95, 0.0, 1.95), 0.0));
    stencils.extend(stencil("PARTS", 0.13, vec3(2.1, 0.0, -1.85), 0.0));
    stencils.extend(stencil("KEEP CLEAR", 0.11, vec3(0.0, 0.0, 2.6), 0.0));
    scene.add(Node::mesh(Geometry::merge(stencils), paint.clone()).shadows(false, true));
    // The output tray: a painted outline.
    let tray = Geometry::merge([
        strip(vec2(-1.75, 0.95), vec2(1.75, 0.95), 0.025),
        strip(vec2(-1.75, 1.55), vec2(1.75, 1.55), 0.025),
        strip(vec2(-1.75, 0.94), vec2(-1.75, 1.56), 0.025),
        strip(vec2(1.75, 0.94), vec2(1.75, 1.56), 0.025),
    ]);
    scene.add(Node::mesh(tray, paint).shadows(false, true));

    // Walls far away, with panel seams and light strips.
    let wall_material = Material::standard(0xc9ccd0).roughness(0.8);
    scene.add(
        Node::mesh(
            Geometry::cuboid(30.0, 7.0, 0.3).translate(0.0, 3.5, -9.0),
            wall_material.clone(),
        )
        .shadows(false, true),
    );
    scene.add(
        Node::mesh(
            Geometry::cuboid(0.3, 7.0, 30.0).translate(-9.0, 3.5, 0.0),
            wall_material.clone(),
        )
        .shadows(false, true),
    );
    let seams = Geometry::merge(
        (-6..=6).map(|i| Geometry::cuboid(0.03, 7.0, 0.04).translate(i as f32 * 2.4, 3.5, -8.84)),
    );
    scene.add(Node::mesh(seams, Material::standard(0xb9bcc0).roughness(0.7)).shadows(false, false));
    let light_strips = Geometry::merge([
        Geometry::rounded_box(16.0, 0.12, 0.05, 0.02, 1).translate(0.0, 2.6, -8.82),
        Geometry::rounded_box(0.05, 0.12, 16.0, 0.02, 1).translate(-8.82, 2.6, 0.0),
    ]);
    let light_strips = scene.add(
        Node::mesh(
            light_strips,
            Material::standard(0xffffff).emissive(0xfff4e6, 0.0),
        )
        .shadows(false, false),
    );
    // Benches along the back wall.
    let bench = Material::standard(0xd7d9dc).roughness(0.5);
    for x in [-4.5f32, 3.2] {
        scene.add(Node::mesh(
            Geometry::rounded_box(2.6, 0.06, 0.8, 0.02, 2).translate(x, 0.92, -7.9),
            bench.clone(),
        ));
        scene.add(Node::mesh(
            Geometry::rounded_box(2.5, 0.85, 0.7, 0.03, 2).translate(x, 0.44, -7.95),
            Material::standard(0xb4b8bd).roughness(0.6),
        ));
        scene.add(Node::mesh(
            Geometry::rounded_box(0.5, 0.35, 0.05, 0.02, 2).translate(x - 0.6, 1.2, -8.1),
            Material::standard(0x1b1c1f).emissive(0x86c6ff, 0.6),
        ));
    }

    // The letter rack: an aluminium frame on a base plate.
    let alu = Material::metal(0xc9ccd1, 0.32);
    scene.add(Node::mesh(
        Geometry::rounded_box(4.3, 0.02, 0.36, 0.008, 2).translate(0.0, 0.01, crate::RACK_Z),
        Material::metal(0x34363b, 0.45),
    ));
    for z in [crate::RACK_Z - 0.14, crate::RACK_Z + 0.14] {
        scene.add(Node::mesh(
            Geometry::rounded_box(4.2, 0.04, 0.04, 0.006, 2).translate(0.0, 0.12, z),
            alu.clone(),
        ));
        scene.add(Node::mesh(
            Geometry::rounded_box(4.2, 0.006, 0.012, 0.002, 1).translate(0.0, 0.142, z),
            Material::metal(0x5b5e64, 0.5),
        ));
    }
    let posts = Geometry::merge((0..8).flat_map(|i| {
        let x = -1.82 + i as f32 * 0.52;
        [crate::RACK_Z - 0.14, crate::RACK_Z + 0.14]
            .map(|z| Geometry::rounded_box(0.04, 0.12, 0.04, 0.006, 1).translate(x, 0.06, z))
    }));
    scene.add(Node::mesh(posts, alu));

    let key = scene.add(Node::light(
        Light::sun(Rgb::hex(0xfff6ea), 0.0, vec3(1.8, 7.0, 4.0)).with_shadow(Shadow {
            map_size: 3072,
            softness: 3.5,
            strength: 0.95,
            bounds: Some(Aabb {
                min: vec3(-3.2, 0.0, -2.2),
                max: vec3(3.2, 3.2, 2.6),
            }),
            ..Default::default()
        }),
    ));
    let fill = scene.add(Node::light(Light::directional(
        Rgb::hex(0xcfe0ff),
        0.0,
        vec3(-4.0, 3.0, -2.0),
    )));
    let ambient = scene.add(Node::light(Light::hemisphere(0xf2f4f6, 0x8a8680, 0.0)));
    Lab {
        key,
        fill,
        ambient,
        light_strips,
    }
}

/// Sets the light level of the lab: 0 is dark blue, 1 is fully lit.
pub fn set_lights(scene: &mut Scene, lab: &Lab, level: f32) {
    scene.environment = environment(level.max(0.04));
    *scene[lab.key].light_mut().expect("key").intensity_mut() = 2.4 * level;
    *scene[lab.fill].light_mut().expect("fill").intensity_mut() = 0.6 * level;
    *scene[lab.ambient]
        .light_mut()
        .expect("ambient")
        .intensity_mut() = level + 0.25;
    let air = Rgb::hex(0x0c1220).lerp(Rgb::hex(0xbfc3c7), level);
    scene.background = Background::Color(air);
    scene.fog = Some(Fog::Exp2 {
        color: air,
        density: 0.045,
    });
    if let Some(m) = scene[lab.light_strips].material_mut() {
        m.emissive_intensity = 2.2 * level;
    }
}
