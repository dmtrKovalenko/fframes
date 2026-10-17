use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex, PoisonError};

use glam::{Mat3, Vec2, Vec3, vec3};
use rayon::prelude::*;

use crate::Rgb;
use crate::color::srgb_to_linear;
use crate::math::smoothstep;

/// The light that surrounds the scene, three.js `scene.environment`.
///
/// Metals reflect the environment, and it lights the shadows. Without it, metal looks black.
#[derive(Debug, Clone, Default)]
pub enum Environment {
    /// No light from the environment. The default.
    #[default]
    None,
    /// A procedural photo studio: a gradient backdrop with rectangular softboxes, like the
    /// three.js `RoomEnvironment`. Metal shows clean highlights in it. Without softboxes it
    /// is a plain sky to ground gradient.
    Studio(Studio),
    /// An equirectangular panorama (`.hdr`, `.jpg`, `.png`), three.js `RGBELoader` with
    /// `EquirectangularReflectionMapping`.
    Map {
        map: Arc<EnvironmentMap>,
        intensity: f32,
        /// The rotation around Y in radians.
        rotation: f32,
    },
}

/// A procedural photo studio, see [`Environment::Studio`] and [`Environment::studio`].
#[derive(Debug, Clone, PartialEq)]
pub struct Studio {
    /// The linear radiance of the backdrop straight up.
    pub sky: Rgb,
    /// The linear radiance of the backdrop at the horizon.
    pub horizon: Rgb,
    /// The linear radiance of the backdrop straight down.
    pub ground: Rgb,
    pub softboxes: Vec<SoftBox>,
    /// The multiplier for all light of the studio. Default 1.
    pub intensity: f32,
    /// The rotation around Y in radians. Turn it to move the reflections over a surface.
    pub rotation: f32,
}

/// A rectangular light in a [`Studio`], as seen from the center of the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct SoftBox {
    /// The unit direction from the scene to the center of the box.
    pub direction: Vec3,
    /// Half the width and half the height of the box, as angles in radians.
    pub half_size: Vec2,
    /// The linear radiance. It is usually well above 1.
    pub color: Rgb,
}

impl SoftBox {
    /// A box in `direction`, `width_degrees` wide and `height_degrees` high. The width runs
    /// parallel to the horizon.
    pub fn new(
        direction: Vec3,
        width_degrees: f32,
        height_degrees: f32,
        color: impl Into<Rgb>,
    ) -> Self {
        Self {
            direction: direction.normalize_or(Vec3::Y),
            half_size: Vec2::new(width_degrees, height_degrees).map(f32::to_radians) * 0.5,
            color: color.into(),
        }
    }

    /// The unit axes of the box: across its width and across its height.
    pub(crate) fn axes(&self) -> (Vec3, Vec3) {
        let u = self.direction.cross(Vec3::Y).normalize_or(Vec3::X);
        (u, u.cross(self.direction))
    }
}

impl Environment {
    /// A neutral dark studio for products.
    ///
    /// It has four softboxes. A large key light is above at the front left, and a strip
    /// light is on the right. A rim strip is behind, and a soft light is straight above.
    pub fn studio() -> Self {
        Environment::Studio(Studio {
            sky: Rgb::linear(0.05, 0.055, 0.065),
            horizon: Rgb::linear(0.12, 0.12, 0.13),
            ground: Rgb::linear(0.02, 0.02, 0.022),
            softboxes: vec![
                SoftBox::new(
                    vec3(-0.6, 0.75, 0.6),
                    50.0,
                    30.0,
                    Rgb::linear(3.2, 3.2, 3.3),
                ),
                SoftBox::new(
                    vec3(1.0, 0.2, 0.15),
                    14.0,
                    70.0,
                    Rgb::linear(2.4, 2.45, 2.6),
                ),
                SoftBox::new(
                    vec3(-0.3, 0.35, -1.0),
                    70.0,
                    8.0,
                    Rgb::linear(1.8, 1.9, 2.1),
                ),
                SoftBox::new(Vec3::Y, 40.0, 40.0, Rgb::linear(0.9, 0.9, 0.95)),
            ],
            intensity: 1.0,
            rotation: 0.0,
        })
    }

    /// A panorama without rotation, see [`EnvironmentMap::decode`].
    pub fn map(map: Arc<EnvironmentMap>, intensity: f32) -> Self {
        Environment::Map {
            map,
            intensity,
            rotation: 0.0,
        }
    }

    /// Sets the brightness multiplier.
    pub fn intensity(mut self, value: f32) -> Self {
        match &mut self {
            Environment::None => {}
            Environment::Studio(Studio { intensity, .. }) | Environment::Map { intensity, .. } => {
                *intensity = value;
            }
        }
        self
    }

    /// Sets the rotation around Y in radians.
    pub fn rotated(mut self, radians: f32) -> Self {
        match &mut self {
            Environment::Studio(Studio { rotation, .. }) | Environment::Map { rotation, .. } => {
                *rotation = radians;
            }
            Environment::None => {}
        }
        self
    }

    /// The light that arrives from `dir`, blurred as a surface of `roughness` reflects it.
    pub fn radiance(&self, dir: Vec3, roughness: f32) -> Vec3 {
        match self {
            Environment::None => Vec3::ZERO,
            Environment::Studio(studio) => studio.radiance(dir, roughness),
            Environment::Map {
                map,
                intensity,
                rotation,
            } => map.sample(rotate_y(dir, -*rotation), roughness) * *intensity,
        }
    }

    /// The baked panorama of a studio, see [`Studio::baked`]. `None` for the other kinds: a
    /// map is a panorama already.
    pub(crate) fn baked(&self) -> Option<Arc<EnvironmentMap>> {
        match self {
            Environment::Studio(studio) => Some(studio.baked()),
            _ => None,
        }
    }

    /// The diffuse light on a surface that faces `normal`. It is the cosine weighted mean of
    /// the incoming light, divided by π.
    pub fn irradiance(&self, normal: Vec3) -> Vec3 {
        match self {
            Environment::None => Vec3::ZERO,
            Environment::Studio(studio) => studio.irradiance(normal),
            Environment::Map {
                map,
                intensity,
                rotation,
            } => map.irradiance(rotate_y(normal, -*rotation)) * *intensity,
        }
    }
}

/// Rotates `v` by `angle` radians around the Y axis.
pub(crate) fn rotate_y(v: Vec3, angle: f32) -> Vec3 {
    if angle == 0.0 {
        v
    } else {
        Mat3::from_rotation_y(angle) * v
    }
}

/// The color of a gradient for a unit direction with height `y`. A larger `softness` gives a
/// wider blend at the horizon.
fn gradient(y: f32, sky: Rgb, horizon: Rgb, ground: Rgb, softness: f32) -> Vec3 {
    let width = 0.08 + softness * 0.6;
    if y >= 0.0 {
        horizon.0.lerp(sky.0, smoothstep(0.0, width + 0.35, y))
    } else {
        horizon.0.lerp(ground.0, smoothstep(0.0, width, -y))
    }
}

/// Returns `true` when a direction is outside a softbox, where the mask of the box is 0.
///
/// `cos_angle` is the cosine of the angle to the center of the box. `a` and `b` are the half
/// angles of the box plus its soft edge. The test is exact: tan²(angle) is
/// tan²(across) + tan²(along), and tan² is convex. Thus beyond the angle √(a² + b²), one of
/// the two angles is past its edge.
fn outside_box(cos_angle: f32, a: f32, b: f32) -> bool {
    a.max(b) < FRAC_PI_2 && cos_angle <= (a * a + b * b).sqrt().cos().max(0.0)
}

/// The number of levels of a baked [`Studio`]. Level `k` shows the studio through the
/// roughness `(k / (BAKED_LEVELS - 1))²`, the level that [`EnvironmentMap::sample`] picks.
const BAKED_LEVELS: usize = 8;
/// The width of the first baked level in texels: 0.35° for each texel. The softest box edge
/// is 0.86° wide.
const BAKED_WIDTH: usize = 1024;

/// The 8 most recent baked studios, keyed by `Studio::bake_key`.
static BAKED: Mutex<Vec<(u64, Arc<EnvironmentMap>)>> = Mutex::new(Vec::new());

impl Studio {
    /// The light of the studio for all directions and roughness values, as a prefiltered
    /// panorama.
    ///
    /// The first call for a distinct studio bakes the panorama. Later calls get it from a
    /// cache. Rotation and intensity apply at sampling time, so they do not cause a new bake.
    /// A reflection then costs one texture lookup instead of the softbox math. The panorama
    /// matches [`Studio::radiance`] to within its texels.
    pub(crate) fn baked(&self) -> Arc<EnvironmentMap> {
        let key = self.bake_key();
        let mut cache = BAKED.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((_, map)) = cache.iter().find(|(k, _)| *k == key) {
            return Arc::clone(map);
        }
        // The bake runs under the lock, because the other threads of a render need the same
        // panorama.
        let map = Arc::new(self.bake());
        if cache.len() >= 8 {
            cache.remove(0);
        }
        cache.push((key, Arc::clone(&map)));
        map
    }

    /// A hash of the fields that change the bake. Rotation and intensity do not change it.
    fn bake_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        let mut add = |values: &[f32]| values.iter().for_each(|v| v.to_bits().hash(&mut hasher));
        for color in [self.sky, self.horizon, self.ground] {
            add(&color.0.to_array());
        }
        for b in &self.softboxes {
            add(&b.direction.to_array());
            add(&b.half_size.to_array());
            add(&b.color.0.to_array());
        }
        hasher.finish()
    }

    fn bake(&self) -> EnvironmentMap {
        let plain = Studio {
            rotation: 0.0,
            intensity: 1.0,
            ..self.clone()
        };
        let levels = (0..BAKED_LEVELS)
            .map(|k| {
                let width = BAKED_WIDTH >> k;
                let height = width / 2;
                let roughness = (k as f32 / (BAKED_LEVELS - 1) as f32).powi(2);
                let mut data = vec![Vec3::ZERO; width * height];
                data.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
                    for (x, out) in row.iter_mut().enumerate() {
                        let uv = Vec2::new(
                            (x as f32 + 0.5) / width as f32,
                            (y as f32 + 0.5) / height as f32,
                        );
                        *out = plain.radiance(EnvironmentMap::dir(uv), roughness);
                    }
                });
                Level {
                    width,
                    height,
                    data,
                }
            })
            .collect();
        EnvironmentMap { levels }
    }

    /// The light that arrives from `dir`, blurred as a surface of `roughness` reflects it.
    fn radiance(&self, dir: Vec3, roughness: f32) -> Vec3 {
        let dir = rotate_y(dir, -self.rotation);
        let blur = roughness * roughness * 1.4;
        let soft = blur + 0.015;
        let mut color = gradient(dir.y, self.sky, self.horizon, self.ground, blur);
        for b in &self.softboxes {
            let d = dir.dot(b.direction);
            let half = b.half_size;
            if d <= -0.2 || outside_box(d, half.x + soft, half.y + soft) {
                continue;
            }
            let (u, v) = b.axes();
            let inside =
                |angle: f32, half: f32| 1.0 - smoothstep(half - soft * 0.5, half + soft, angle);
            let mask = inside(dir.dot(u).atan2(d).abs(), half.x)
                * inside(dir.dot(v).atan2(d).abs(), half.y);
            if mask > 0.0 {
                // The soft edge spreads the light over a larger area. This keeps the total
                // energy of the box.
                let energy = (half.x * half.y) / ((half.x + soft * 0.75) * (half.y + soft * 0.75));
                color += b.color.0 * mask * energy;
            }
        }
        color * self.intensity
    }

    /// The diffuse light on a surface that faces `normal`, see [`Environment::irradiance`].
    fn irradiance(&self, normal: Vec3) -> Vec3 {
        let normal = rotate_y(normal, -self.rotation);
        let mut color = gradient(normal.y, self.sky, self.horizon, self.ground, 1.0);
        for b in &self.softboxes {
            let solid_angle = 4.0 * b.half_size.x.sin() * b.half_size.y.sin();
            color += b.color.0 * (solid_angle * normal.dot(b.direction).max(0.0) / PI);
        }
        color * self.intensity
    }
}

/// A prefiltered equirectangular panorama: a chain of levels, each one blurrier and half
/// the size of the one before. Rough surfaces read the blurry levels.
pub struct EnvironmentMap {
    levels: Vec<Level>,
}

impl std::fmt::Debug for EnvironmentMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnvironmentMap")
            .field("levels", &self.levels.len())
            .finish()
    }
}

/// One level of an [`EnvironmentMap`]: linear RGB texels, row by row from the top.
struct Level {
    width: usize,
    height: usize,
    data: Vec<Vec3>,
}

impl Level {
    /// The bilinear sample at `uv`. U wraps around, v stops at the poles.
    fn sample(&self, uv: Vec2) -> Vec3 {
        let x = uv.x * self.width as f32 - 0.5;
        let y = (uv.y * self.height as f32 - 0.5).clamp(0.0, self.height as f32 - 1.0);
        let x0 = x.floor();
        let y0 = y.floor();
        let (fx, fy) = (x - x0, y - y0);
        let xi = (x0 as i64).rem_euclid(self.width as i64) as usize;
        let xi1 = (xi + 1) % self.width;
        let yi = y0 as usize;
        let yi1 = (yi + 1).min(self.height - 1);
        let at = |x: usize, y: usize| self.data[y * self.width + x];
        at(xi, yi)
            .lerp(at(xi1, yi), fx)
            .lerp(at(xi, yi1).lerp(at(xi1, yi1), fx), fy)
    }

    /// The level at half the size, filtered with a 4x4 tent.
    fn downsample(&self) -> Level {
        const TENT: [f32; 4] = [1.0, 3.0, 3.0, 1.0];
        let width = (self.width / 2).max(2);
        let height = (self.height / 2).max(1);
        let mut data = vec![Vec3::ZERO; width * height];
        for y in 0..height {
            for x in 0..width {
                let mut sum = Vec3::ZERO;
                for (dy, wy) in (-1i64..=2).zip(TENT) {
                    for (dx, wx) in (-1i64..=2).zip(TENT) {
                        let sx = (x as i64 * 2 + dx).rem_euclid(self.width as i64) as usize;
                        let sy = (y as i64 * 2 + dy).clamp(0, self.height as i64 - 1) as usize;
                        sum += self.data[sy * self.width + sx] * (wx * wy);
                    }
                }
                // The 16 weights add up to 64.
                data[y * width + x] = sum / 64.0;
            }
        }
        Level {
            width,
            height,
            data,
        }
    }
}

impl EnvironmentMap {
    /// Decodes an equirectangular panorama. A Radiance `.hdr` file is linear, an 8-bit
    /// image is sRGB.
    pub fn decode(bytes: &[u8]) -> Result<Self, image::ImageError> {
        let image = image::load_from_memory(bytes)?;
        let is_float = matches!(
            image,
            image::DynamicImage::ImageRgb32F(_) | image::DynamicImage::ImageRgba32F(_)
        );
        let to_linear = |c: f32| if is_float { c } else { srgb_to_linear(c) };
        let rgb = image.to_rgb32f();
        let data = rgb
            .pixels()
            .map(|p| Vec3::from(p.0.map(to_linear)))
            .collect();
        Ok(Self::from_linear(
            rgb.width() as usize,
            rgb.height() as usize,
            data,
        ))
    }

    /// A panorama from linear RGB texels, row by row from the top. Levels wider than 1024
    /// texels are halved until they fit. The smallest level is 8 texels wide or less.
    pub fn from_linear(width: usize, height: usize, data: Vec<Vec3>) -> Self {
        let mut level = Level {
            width,
            height,
            data,
        };
        while level.width > 1024 {
            level = level.downsample();
        }
        let mut levels = vec![level];
        while let Some(last) = levels.last().filter(|l| l.width > 8) {
            levels.push(last.downsample());
        }
        Self { levels }
    }

    /// The prefiltered levels as width, height and linear RGB texels, sharpest first.
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    pub(crate) fn levels(&self) -> impl Iterator<Item = (usize, usize, &[Vec3])> {
        self.levels
            .iter()
            .map(|l| (l.width, l.height, l.data.as_slice()))
    }

    /// The panorama position of a direction.
    fn uv(dir: Vec3) -> Vec2 {
        let dir = dir.normalize_or(Vec3::Y);
        Vec2::new(
            dir.z.atan2(dir.x) / TAU + 0.5,
            0.5 - dir.y.clamp(-1.0, 1.0).asin() / PI,
        )
    }

    /// The unit direction of a panorama position, the inverse of [`Self::uv`].
    fn dir(uv: Vec2) -> Vec3 {
        let azimuth = (uv.x - 0.5) * TAU;
        let elevation = (0.5 - uv.y) * PI;
        let (sin_e, cos_e) = elevation.sin_cos();
        let (sin_a, cos_a) = azimuth.sin_cos();
        Vec3::new(cos_e * cos_a, sin_e, cos_e * sin_a)
    }

    /// The light from `dir`, blurred as a surface of `roughness` reflects it. A mirror
    /// reads level 0, a rough surface the smallest levels.
    pub(crate) fn sample(&self, dir: Vec3, roughness: f32) -> Vec3 {
        let uv = Self::uv(dir);
        let last = self.levels.len() - 1;
        let lod = (roughness.clamp(0.0, 1.0).sqrt() * last as f32).min(last as f32);
        let l0 = lod.floor() as usize;
        let l1 = (l0 + 1).min(last);
        self.levels[l0]
            .sample(uv)
            .lerp(self.levels[l1].sample(uv), lod - l0 as f32)
    }

    /// The diffuse light on a surface that faces `normal`, from the smallest level.
    fn irradiance(&self, normal: Vec3) -> Vec3 {
        self.levels[self.levels.len() - 1].sample(Self::uv(normal))
    }
}
