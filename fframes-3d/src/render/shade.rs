//! Physically based shading of one surface point.

use std::f32::consts::PI;

use glam::{Vec2, Vec3, Vec4};

use super::prepare::{Draw, LightKind, Lights};
use super::reflection::ReflectionMap;
use super::shadow::ShadowMap;
use crate::material::{Pattern, Shading};
use crate::math::smoothstep;
use crate::scene::{Background, Fog};
use crate::{Blending, Environment};

/// What the shading of one view needs: the lights, the environment and the camera.
pub(crate) struct ShadeContext<'a> {
    pub lights: &'a Lights,
    /// The shadow map of each of [`Lights::shadow_casters`], `None` where nothing casts a
    /// shadow.
    pub shadows: &'a [Option<ShadowMap>],
    pub environment: &'a Environment,
    /// The radiance of the studio as a panorama, see [`crate::Environment::baked`].
    pub baked: Option<&'a crate::EnvironmentMap>,
    pub fog: Option<Fog>,
    pub background: &'a Background,
    pub camera_position: Vec3,
    /// The view direction of an orthographic camera.
    pub ortho_forward: Option<Vec3>,
    /// The angle that one pixel covers, to antialias procedural patterns.
    pub pixel_angle: f32,
    /// The scene mirrored in its reflector, see [`crate::Reflector`].
    pub reflection: Option<&'a ReflectionMap>,
    /// The size of the view in pixels. The reflection has the same alignment.
    pub viewport: Vec2,
}

impl ShadeContext<'_> {
    /// The shadow map at `index` in [`ShadeContext::shadows`].
    fn shadow(&self, index: Option<usize>) -> Option<&ShadowMap> {
        self.shadows.get(index?)?.as_ref()
    }

    /// The light that arrives from `dir`, blurred for `roughness`.
    pub(crate) fn env_radiance(&self, dir: Vec3, roughness: f32) -> Vec3 {
        match (self.environment, self.baked) {
            (Environment::Studio(studio), Some(map)) => {
                map.sample(
                    crate::environment::rotate_y(dir, -studio.rotation),
                    roughness,
                ) * studio.intensity
            }
            (environment, _) => environment.radiance(dir, roughness),
        }
    }
}

/// A point on a triangle, interpolated from its vertices.
pub(crate) struct Surface {
    pub position: Vec3,
    pub normal: Vec3,
    pub face_normal: Vec3,
    pub uv: Vec2,
    pub color: Vec4,
    /// The UV units that one meter of the triangle spans.
    pub uv_per_meter: f32,
    /// How much the smooth normal turns to the next pixel right and down: the sum of the
    /// squared differences. The specular antialiasing uses it.
    pub normal_variation: f32,
    /// The center of the pixel on the view.
    pub screen: Vec2,
}

/// The premultiplied linear color and the coverage of a surface point.
pub(crate) fn shade(ctx: &ShadeContext, draw: &Draw, s: &Surface) -> Vec4 {
    let m = draw.material;
    let to_camera = ctx.camera_position - s.position;
    let distance = to_camera.length();
    let v = match ctx.ortho_forward {
        Some(forward) => -forward,
        None => to_camera / distance.max(1e-6),
    };
    let face = if s.face_normal.dot(v) < 0.0 {
        -s.face_normal
    } else {
        s.face_normal
    };
    let mut n = if m.flat_shading { face } else { s.normal };
    // a smooth normal can point away from the camera near a silhouette
    if n.dot(face) < 0.0 {
        n = -n;
    }

    // The footprint of the pixel in UV units. Surfaces seen at a low angle get blurrier.
    let n_dot_v = n.dot(v).max(1e-4);
    let footprint = distance * ctx.pixel_angle / n_dot_v.max(0.03).sqrt() * s.uv_per_meter;
    let sample = |map: &crate::Texture| map.sample_lod(s.uv, map.lod(footprint));

    let mut base = m.color.0;
    let mut alpha = m.opacity;
    if draw.has_color {
        base *= s.color.truncate();
        alpha *= s.color.w;
    }
    if draw.has_uv
        && let Some(map) = &m.map
    {
        let texel = sample(map);
        base *= texel.truncate();
        alpha *= texel.w;
    }

    let mut emissive = m.emissive.0 * m.emissive_intensity;
    if draw.has_uv
        && let Some(map) = &m.emissive_map
    {
        emissive *= sample(map).truncate();
    }

    if m.rim != crate::Rgb::BLACK {
        emissive += m.rim.0 * (1.0 - n_dot_v).powf(m.rim_power);
    }
    if let Some(Pattern::Grid {
        cell,
        line_width,
        color,
        fade_distance,
    }) = &m.pattern
    {
        emissive += color.0
            * grid(
                s.position,
                face,
                *cell,
                *line_width,
                distance,
                n_dot_v,
                ctx.pixel_angle,
            )
            * (1.0 - smoothstep(fade_distance * 0.5, *fade_distance, distance));
    }

    let mut rgb = match m.shading {
        Shading::Unlit => base + emissive,
        Shading::Normals => n * 0.5 + 0.5,
        Shading::ShadowCatcher => {
            let mut lit = 1.0;
            for light in &ctx.lights.direct {
                if let LightKind::Directional { direction, shadow } = &light.kind
                    && let Some(map) = ctx.shadow(*shadow)
                {
                    lit *= map.visibility(s.position, face, face.dot(-*direction));
                }
            }
            return Vec4::new(0.0, 0.0, 0.0, (1.0 - lit) * m.opacity);
        }
        Shading::Standard => {
            standard(ctx, draw, s, base, n, face, v, n_dot_v, footprint) + emissive
        }
    };

    if m.fog
        && let Some(fog) = ctx.fog
    {
        let f = fog.factor(distance);
        rgb = match m.blending {
            Blending::Additive => rgb * (1.0 - f),
            Blending::Normal => rgb.lerp(fog.color().0, f),
        };
    }

    match m.blending {
        // additive light has no coverage
        Blending::Additive => (rgb * alpha).extend(0.0),
        Blending::Normal => (rgb * alpha).extend(alpha),
    }
}

/// The metal and roughness PBR color of a surface: the direct light, then the ambient
/// light and the environment. `n` is the shading normal, `face` the normal of the
/// triangle, `v` the direction to the camera.
#[allow(clippy::too_many_arguments)]
fn standard(
    ctx: &ShadeContext,
    draw: &Draw,
    s: &Surface,
    base: Vec3,
    n: Vec3,
    face: Vec3,
    v: Vec3,
    n_dot_v: f32,
    footprint: f32,
) -> Vec3 {
    let m = draw.material;
    let mut metalness = m.metalness;
    let mut roughness = m.roughness;
    if draw.has_uv
        && let Some(map) = &m.metal_rough_map
    {
        let texel = map.sample_lod(s.uv, map.lod(footprint));
        roughness *= texel.y;
        metalness *= texel.z;
    }
    let mut roughness = roughness.clamp(0.045, 1.0);
    let metalness = metalness.clamp(0.0, 1.0);
    let mut alpha = roughness * roughness;
    let clearcoat = m.clearcoat.clamp(0.0, 1.0);
    let mut cc_roughness = m.clearcoat_roughness;
    let mut cc_alpha = cc_roughness.clamp(0.045, 1.0).powi(2);
    // Specular antialiasing: a normal that turns in the pixel widens the highlight, so it
    // does not sparkle (Kaplanyan 2016, with the constants of Filament).
    let kernel = (0.3 * s.normal_variation).min(0.04);
    if kernel > 0.0 {
        alpha = (alpha * alpha + kernel).min(1.0).sqrt();
        roughness = alpha.sqrt();
        cc_alpha = (cc_alpha * cc_alpha + kernel).min(1.0).sqrt();
        cc_roughness = cc_alpha.sqrt();
    }
    let f0 = Vec3::splat(0.04).lerp(base, metalness);
    let diffuse = base * (1.0 - metalness);

    let mut direct = Vec3::ZERO;
    for light in &ctx.lights.direct {
        let (l, radiance) = match &light.kind {
            LightKind::Directional { direction, shadow } => {
                let l = -*direction;
                let mut radiance = light.color;
                let n_dot_l = face.dot(l);
                if draw.receive_shadow
                    && n_dot_l > 0.0
                    && let Some(map) = ctx.shadow(*shadow)
                {
                    radiance *= map.visibility(s.position, face, n_dot_l);
                }
                (l, radiance)
            }
            LightKind::Point {
                position,
                distance,
                decay,
            } => {
                let d = *position - s.position;
                let dist = d.length();
                (
                    d / dist.max(1e-6),
                    light.color * attenuation(dist, *distance, *decay),
                )
            }
            LightKind::Spot {
                position,
                direction,
                distance,
                decay,
                cos_outer,
                cos_inner,
            } => {
                let d = *position - s.position;
                let dist = d.length();
                let l = d / dist.max(1e-6);
                let cone = smoothstep(*cos_outer, *cos_inner, (-l).dot(*direction));
                (l, light.color * attenuation(dist, *distance, *decay) * cone)
            }
        };
        let n_dot_l = n.dot(l);
        if n_dot_l <= 0.0 || radiance.max_element() <= 0.0 {
            continue;
        }
        let h = (l + v).normalize_or(n);
        let n_dot_h = n.dot(h).max(0.0);
        let v_dot_h = v.dot(h).max(0.0);
        let fresnel = f0 + (Vec3::ONE - f0) * (1.0 - v_dot_h).powi(5);
        let specular = fresnel * (ggx(n_dot_h, alpha) * smith_visibility(n_dot_l, n_dot_v, alpha));
        let mut lit = (diffuse * (Vec3::ONE - fresnel) / PI + specular) * radiance * n_dot_l;
        if clearcoat > 0.0 {
            let fc = (0.04 + 0.96 * (1.0 - v_dot_h).powi(5)) * clearcoat;
            let coat = ggx(n_dot_h, cc_alpha) * smith_visibility(n_dot_l, n_dot_v, cc_alpha) * fc;
            lit = lit * (1.0 - fc) + radiance * (coat * n_dot_l);
        }
        direct += lit;
    }

    // ambient and hemisphere lights (three.js divides their irradiance by pi)
    let mut irradiance = ctx.lights.ambient / PI;
    if let Some((sky, ground)) = ctx.lights.hemisphere {
        irradiance += ground.lerp(sky, n.y * 0.5 + 0.5) / PI;
    }
    let env = ctx.environment;
    let has_env = !matches!(env, Environment::None);
    if has_env {
        irradiance += env.irradiance(n) * m.env_intensity;
    }
    let mut indirect = diffuse * irradiance;

    if has_env {
        let mirror = (-v).reflect(n);
        // the main direction of a rough reflection moves to the normal
        let r = mirror.lerp(n, alpha).normalize_or(n);
        // reflections from below the surface are hidden
        let horizon = (1.0 + r.dot(face)).clamp(0.0, 1.0).powi(2);
        let (scale, bias) = env_brdf(roughness, n_dot_v);
        let reflection = ctx.reflection.filter(|_| m.reflection > 0.0);
        let uv = s.screen / ctx.viewport;
        let env = ctx.env_radiance(r, roughness) * m.env_intensity;
        let mut radiance = env * horizon;
        if let Some(map) = reflection {
            // the reflection of the scene over the environment, where it covers the pixel
            let p = map.sample(uv, roughness);
            radiance = radiance.lerp((p.truncate() + env * (1.0 - p.w)) * horizon, m.reflection);
        }
        indirect += radiance * (f0 * scale + Vec3::splat(bias));
        if clearcoat > 0.0 {
            let (cs, cb) = env_brdf(cc_roughness, n_dot_v);
            let fc = (0.04 * cs + cb) * clearcoat;
            let mut coat = ctx.env_radiance(mirror, cc_roughness) * m.env_intensity;
            if let Some(map) = reflection {
                let p = map.sample(uv, cc_roughness);
                coat = coat.lerp(p.truncate() + coat * (1.0 - p.w), m.reflection);
            }
            indirect = indirect * (1.0 - fc) + coat * (horizon * fc);
        }
    } else {
        // without an environment, ambient light still gives a little sheen
        let (scale, bias) = env_brdf(roughness, n_dot_v);
        indirect += irradiance * (f0 * scale + Vec3::splat(bias)) * 0.5;
    }

    direct + indirect
}

/// three.js `getDistanceAttenuation`.
fn attenuation(distance: f32, cutoff: f32, decay: f32) -> f32 {
    let mut att = 1.0 / distance.max(0.01).powf(decay);
    if cutoff > 0.0 {
        let x = (1.0 - (distance / cutoff).powi(4)).clamp(0.0, 1.0);
        att *= x * x;
    }
    att
}

#[inline]
fn ggx(n_dot_h: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    a2 / (PI * d * d)
}

/// The height correlated Smith visibility, with the 1 / (4 `NdotL` `NdotV`) term.
#[inline]
fn smith_visibility(n_dot_l: f32, n_dot_v: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    let gv = n_dot_l * (n_dot_v * n_dot_v * (1.0 - a2) + a2).sqrt();
    let gl = n_dot_v * (n_dot_l * n_dot_l * (1.0 - a2) + a2).sqrt();
    0.5 / (gv + gl).max(1e-6)
}

/// The analytic fit of the split sum environment BRDF (Karis, for mobile).
#[inline]
fn env_brdf(roughness: f32, n_dot_v: f32) -> (f32, f32) {
    let c0 = Vec4::new(-1.0, -0.0275, -0.572, 0.022);
    let c1 = Vec4::new(1.0, 0.0425, 1.04, -0.04);
    let r = c0 * roughness + c1;
    let a004 = (r.x * r.x).min((-9.28 * n_dot_v).exp2()) * r.x + r.y;
    (a004 * -1.04 + r.z, a004 * 1.04 + r.w)
}

/// The coverage of the grid lines at a point, antialiased by the pixel footprint.
fn grid(
    p: Vec3,
    n: Vec3,
    cell: f32,
    line_width: f32,
    distance: f32,
    n_dot_v: f32,
    pixel_angle: f32,
) -> f32 {
    let a = n.abs();
    let (u, v) = if a.y >= a.x && a.y >= a.z {
        (p.x, p.z)
    } else if a.x >= a.z {
        (p.y, p.z)
    } else {
        (p.x, p.y)
    };
    let footprint = (distance * pixel_angle / n_dot_v.max(0.2)).max(1e-5);
    let line = |c: f32| {
        let d = ((c / cell).fract().abs() - 0.5).abs();
        // the distance to the nearest line
        let d = (0.5 - d) * cell;
        let half = line_width * 0.5;
        let coverage = ((half + footprint * 0.5 - d) / footprint).clamp(0.0, 1.0);
        // lines thinner than a pixel get dimmer, so they do not alias
        coverage * (line_width / footprint).min(1.0)
    };
    let (lu, lv) = (line(u), line(v));
    lu + lv - lu * lv
}
