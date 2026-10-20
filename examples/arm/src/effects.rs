//! Effects that the film builds itself from the building blocks of fframes_three_d: random
//! numbers, camera shake, a light beam in hazy air, sparks and a printed grid.

use fframes::three_d::math::smoothstep;
use fframes::three_d::*;

/// Maps an integer to a pseudo-random value in `0..1`. The same input always gives the same
/// value.
pub fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Linear progress of `t` from `start` to `end`, clamped to `0..1`.
pub fn progress(t: f32, start: f32, end: f32) -> f32 {
    ((t - start) / (end - start)).clamp(0.0, 1.0)
}

/// Smooth 1D gradient noise in `-1..1`. Each integer `x` has a random slope and the value 0,
/// and a quintic curve blends the slopes of the two neighbours, so a camera that it moves
/// does not stop and start again.
pub fn noise(seed: u32, x: f32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let cell = (i as i32 as u32).wrapping_add(seed.wrapping_mul(1013));
    let g0 = hash(cell) * 2.0 - 1.0;
    let g1 = hash(cell.wrapping_add(1)) * 2.0 - 1.0;
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let (a, b) = (g0 * f, g1 * (f - 1.0));
    // the largest value is 0.5
    2.0 * (a + (b - a) * u)
}

/// A pseudo-random unit vector. The same `seed` always gives the same vector.
pub fn direction(seed: u32) -> Vec3 {
    let u = hash(seed.wrapping_mul(2).wrapping_add(1)) * 2.0 - 1.0;
    let phi = hash(seed.wrapping_mul(2).wrapping_add(2)) * std::f32::consts::TAU;
    let r = (1.0 - u * u).max(0.0).sqrt();
    Vec3::new(r * phi.cos(), u, r * phi.sin())
}

/// Shakes `camera` after the hits at `(time, strength)`: it turns by up to `strength`
/// degrees and the shake dies out in about 0.4 seconds. The camera keeps its lens, only the
/// direction of the view changes.
pub fn shake(camera: Camera, t: f32, hits: impl IntoIterator<Item = (f32, f32)>) -> Camera {
    let amount: f32 = hits
        .into_iter()
        .filter(|&(at, _)| t >= at)
        .map(|(at, strength)| strength * (-(t - at) * 10.0).exp())
        .sum();
    if amount < 1e-3 {
        return camera;
    }
    // Two noise curves for the yaw and the pitch, at 18 turns per second.
    let yaw = deg(amount * noise(1, t * 18.0));
    let pitch = deg(amount * noise(2, t * 18.0));
    let forward = camera.forward();
    let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
    let up = right.cross(forward);
    let turned = Quat::from_axis_angle(up, yaw) * Quat::from_axis_angle(right, pitch) * forward;
    let target = camera.position + turned;
    camera.look_at(target)
}

/// A beam of light in hazy air, along +Z from the lamp at the origin to a circle of
/// `radius` at `length`. Draw it with [`Material::glow`]: 16 cones inside each other add
/// up to a beam that is bright in the middle and soft at the sides, and their vertex colors
/// fade it out along its length.
pub fn beam(length: f32, radius: f32) -> Geometry {
    const CONES: usize = 16;
    let rings = 16;
    let cones = (0..CONES).map(|k| {
        let scale = 1.0 - k as f32 / CONES as f32;
        // A lathe along -Y, turned so that -Y becomes +Z. The profile goes from the wide end
        // up to the lamp, so the faces point out, as in three.js.
        let profile: Vec<Vec2> = (0..=rings)
            .rev()
            .map(|i| {
                let along = i as f32 / rings as f32;
                vec2((0.004 + radius * along) * scale, -length * along)
            })
            .collect();
        Geometry::lathe(&profile, 32).rotate_x(deg(-90.0))
    });
    let mut geometry = Geometry::merge(cones);
    geometry.colors = geometry
        .positions
        .iter()
        .map(|p| {
            let along = (p.z / length).clamp(0.0, 1.0);
            // Bright near the lamp, gone at the end of the beam.
            let fade = (1.0 - along).powf(1.6) * smoothstep(0.0, 0.06, along) / CONES as f32;
            vec4(fade, fade, fade, 1.0)
        })
        .collect();
    geometry
}

/// Sparks thrown from `at` at the time `start`, `count` of them with up to `speed` meters
/// per second. They fly on a ballistic path, bounce off the floor once and cool down from
/// white to orange. Nothing is left after a second.
///
/// Each spark is a streak: dots along the path it flew in the last `trail` seconds, fading
/// out towards its tail.
pub fn sparks(
    at: Vec3,
    start: f32,
    speed: f32,
    count: u32,
    seed: u32,
    t: f32,
    trail: f32,
) -> Vec<Particle> {
    const DOTS: usize = 8;
    if !(0.0..1.0 + trail).contains(&(t - start)) {
        return Vec::new();
    }
    (0..count)
        .flat_map(|i| (0..DOTS).map(move |k| (i, k)))
        .filter_map(|(i, k)| {
            let tail = k as f32 / DOTS as f32;
            let age = t - start - tail * trail;
            if age < 0.0 {
                return None;
            }
            let id = seed.wrapping_mul(7919).wrapping_add(i);
            let mut v = direction(id) * speed * (0.35 + 0.65 * hash(id ^ 0x5bd1));
            // Mostly up and out of the floor.
            v.y = v.y.abs() * 0.8 + 0.3;
            let life = 0.35 + 0.6 * hash(id ^ 0x2c1b);
            let x = age / life;
            if x >= 1.0 {
                return None;
            }
            let mut p = at + v * age + Vec3::Y * (-4.9 * age * age);
            if p.y < 0.005 {
                p.y = 0.005 + (0.005 - p.y) * 0.25;
            }
            let heat = 1.0 - x;
            let color = Rgb::hex(0xff5a10).lerp(Rgb::hex(0xfff2c0), heat * heat)
                * (2.0 + 10.0 * heat)
                * (1.0 - tail);
            Some(Particle::new(p, 0.003 + 0.004 * heat, color).opacity(heat * (1.0 - tail)))
        })
        .collect()
}

/// A printed grid on a mat: thin lines every `cell` meters and thicker ones every
/// `major` cells, on a texture that repeats every `major * cell` meters. The texture is
/// mipmapped, so the lines do not shimmer far away.
pub fn grid_texture(cell: f32, major: u32, ink: Rgb, paper: Rgb) -> Texture {
    let size = 512;
    let cells = major as f32;
    // How much of a line of `width` texels covers the texel at `c`, in lines per texture
    // when `texels` is the texture size, in cells when it is the size of a cell.
    let line = |c: f32, texels: f32, width: f32| {
        let f = c.fract();
        let d = f.min(1.0 - f) * texels;
        1.0 - smoothstep(width * 0.5 - 0.75, width * 0.5 + 0.75, d)
    };
    Texture::from_fn(size, size, |u, v| {
        let cell_texels = size as f32 / cells;
        let minor = line(u * cells, cell_texels, 3.0).max(line(v * cells, cell_texels, 3.0));
        let major = line(u, size as f32, 10.0).max(line(v, size as f32, 10.0));
        let ink_amount = (minor * 0.55).max(major);
        paper.lerp(ink, ink_amount)
    })
    .repeat(1.0 / (cell * cells), 1.0 / (cell * cells))
}
