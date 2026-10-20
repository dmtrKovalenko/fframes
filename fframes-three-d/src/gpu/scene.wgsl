// The geometry, background and particle passes of the GPU renderer.
// The shading follows `render/shade.rs` function by function, so both renderers agree.

const PI: f32 = 3.14159265;

struct Light {
    // rgb, kind (0 directional, 1 point, 2 spot)
    color: vec4f,
    // xyz, cutoff distance
    position: vec4f,
    // xyz, decay
    direction: vec4f,
    // cos outer, cos inner, shadow map index (-1 for none), 0
    params: vec4f,
}

struct SoftBox {
    // xyz direction, solid angle
    direction: vec4f,
    // rgb, 0
    color: vec4f,
}

struct Shadow {
    view_proj: mat4x4f,
    // world size of a texel, depth per texel, softness (texels), strength
    params: vec4f,
    // bias (texels), map size, 0, 0
    params2: vec4f,
}

struct Frame {
    view_proj: mat4x4f,
    inv_view_proj: mat4x4f,
    // `view_proj` for positions relative to the camera
    view_proj_rel: mat4x4f,
    // xyz, 1 for an orthographic camera
    camera: vec4f,
    // xyz, 0
    forward: vec4f,
    // width, height, focal length (pixels per unit at distance 1), near
    viewport: vec4f,
    // rgb, 1 with a hemisphere light
    ambient: vec4f,
    hemi_sky: vec4f,
    hemi_ground: vec4f,
    // rgb, kind (0 none, 1 linear, 2 exp2)
    fog: vec4f,
    // near, far, density, 0
    fog_params: vec4f,
    // kind (0 none, 1 studio, 2 map), intensity, rotation, softbox count
    env: vec4f,
    env_sky: vec4f,
    env_horizon: vec4f,
    env_ground: vec4f,
    // kind (0 transparent, 1 color, 2 environment), blur, 0, 0
    background: vec4f,
    background_color: vec4f,
    // light count, environment map levels, reflection levels (0 without a reflection), 0
    counts: vec4f,
    // the plane of the reflector: xyz normal, w offset
    clip_plane: vec4f,
    lights: array<Light, 16>,
    softboxes: array<SoftBox, 8>,
    shadows: array<Shadow, 2>,
}

struct Draw {
    model: mat4x4f,
    normal_matrix: mat4x4f,
    // rgb, opacity
    color: vec4f,
    // rgb times intensity, environment intensity
    emissive: vec4f,
    // metalness, roughness, clearcoat, clearcoat roughness
    pbr: vec4f,
    // flat shading, additive, fog, blended
    flags: vec4f,
    // has map, has emissive map, has metal rough map, receives shadows
    maps: vec4f,
    // has uv, has color, reflection, 0
    attrs: vec4f,
    // the uv repeat and offset of each map: color, emissive, metal rough, normal, occlusion
    uv_transforms: array<vec4f, 5>,
    // has normal map, normal scale, has ao map, 0
    detail: vec4f,
}

// The pipeline constants of a material (`Variant` in `mod.rs`). A material compiles only the
// paths that it uses. This keeps the shader small enough to run at full SIMD width. The
// background and particle pipelines use the default values.
override SHADING: u32 = 0u; // 0 standard, 1 unlit, 2 shadow catcher, 3 normals
override ENVIRONMENT: u32 = 255u; // 0 none, 1 studio, 2 map, 255 from the frame
override CLEARCOAT: bool = true;
override TEXTURED: bool = true;
override FOG: bool = true;
override LIGHTS: u32 = 255u; // the light count, 255 from the frame
override REFLECTION: bool = false; // the material shows the reflection of the scene
override REFLECTION_PASS: bool = false; // draws the reflection: nothing behind the reflector
override NORMAL_MAP: bool = false;

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var shadow_maps: texture_depth_2d_array;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(0) @binding(3) var env_tex: texture_2d<f32>;
@group(0) @binding(4) var env_sampler: sampler;
// The scene in the mirror of the reflector. The blurred levels are for rough surfaces.
@group(0) @binding(5) var reflection_tex: texture_2d<f32>;
@group(0) @binding(6) var reflection_sampler: sampler;

@group(1) @binding(0) var<uniform> draw: Draw;
@group(1) @binding(1) var map_tex: texture_2d<f32>;
@group(1) @binding(2) var emissive_tex: texture_2d<f32>;
@group(1) @binding(3) var mr_tex: texture_2d<f32>;
@group(1) @binding(4) var tex_sampler: sampler;
@group(1) @binding(5) var normal_tex: texture_2d<f32>;
@group(1) @binding(6) var ao_tex: texture_2d<f32>;

// The smoothstep of `render/shade.rs`. The formula is explicit, so that both renderers
// compute the same value.
fn smooth_step(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// The largest component of `v`.
fn largest(v: vec3f) -> f32 {
    return max(v.x, max(v.y, v.z));
}

fn normalize_or(v: vec3f, fallback: vec3f) -> vec3f {
    let l = length(v);
    if (l > 1e-8) {
        return v / l;
    }
    return fallback;
}

// The environment.

fn rotate_y(v: vec3f, angle: f32) -> vec3f {
    if (angle == 0.0) {
        return v;
    }
    let s = sin(angle);
    let c = cos(angle);
    return vec3f(c * v.x + s * v.z, v.y, -s * v.x + c * v.z);
}

fn gradient(y: f32, softness: f32) -> vec3f {
    let width = 0.08 + softness * 0.6;
    if (y >= 0.0) {
        return mix(frame.env_horizon.rgb, frame.env_sky.rgb, smooth_step(0.0, width + 0.35, y));
    }
    return mix(frame.env_horizon.rgb, frame.env_ground.rgb, smooth_step(0.0, width, -y));
}

fn equirect_uv(dir: vec3f) -> vec2f {
    let d = normalize_or(dir, vec3f(0.0, 1.0, 0.0));
    return vec2f(atan2(d.z, d.x) / (2.0 * PI) + 0.5, 0.5 - asin(clamp(d.y, -1.0, 1.0)) / PI);
}

fn light_count() -> i32 {
    if (LIGHTS == 255u) {
        return i32(frame.counts.x);
    }
    return i32(LIGHTS);
}

fn env_kind() -> i32 {
    if (ENVIRONMENT == 255u) {
        return i32(frame.env.x);
    }
    return i32(ENVIRONMENT);
}

fn env_radiance(dir_in: vec3f, roughness: f32) -> vec3f {
    let kind = env_kind();
    let intensity = frame.env.y;
    if (kind == 1 || kind == 2) {
        // A panorama: the baked studio or the environment map.
        let dir = rotate_y(dir_in, -frame.env.z);
        let levels = frame.counts.y;
        let lod = min(sqrt(clamp(roughness, 0.0, 1.0)) * (levels - 1.0), levels - 1.0);
        return textureSampleLevel(env_tex, env_sampler, equirect_uv(dir), lod).rgb * intensity;
    }
    return vec3f(0.0);
}

// The reflection of the scene at a pixel position, blurred for `roughness`. Returns the
// premultiplied color and the coverage.
fn planar_reflection(screen: vec2f, roughness: f32) -> vec4f {
    let uv = screen / frame.viewport.xy;
    let last = frame.counts.z - 1.0;
    let lod = min(sqrt(clamp(roughness, 0.0, 1.0)) * last, last);
    return textureSampleLevel(reflection_tex, reflection_sampler, uv, lod);
}

fn env_irradiance(n_in: vec3f) -> vec3f {
    let kind = env_kind();
    let intensity = frame.env.y;
    let n = rotate_y(n_in, -frame.env.z);
    if (kind == 1) {
        var color = gradient(n.y, 1.0);
        let count = i32(frame.env.w);
        for (var i = 0; i < count; i++) {
            let b = &frame.softboxes[i];
            let direction = b.direction;
            color += b.color.rgb * (direction.w * max(dot(n, direction.xyz), 0.0) / PI);
        }
        return color * intensity;
    }
    if (kind == 2) {
        return textureSampleLevel(env_tex, env_sampler, equirect_uv(n), frame.counts.y - 1.0).rgb * intensity;
    }
    return vec3f(0.0);
}

// The lighting.

fn attenuation(distance: f32, cutoff: f32, decay: f32) -> f32 {
    var att = 1.0 / pow(max(distance, 0.01), decay);
    if (cutoff > 0.0) {
        let x = clamp(1.0 - pow(distance / cutoff, 4.0), 0.0, 1.0);
        att = att * x * x;
    }
    return att;
}

fn ggx(n_dot_h: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

fn smith_visibility(n_dot_l: f32, n_dot_v: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    let gv = n_dot_l * sqrt(n_dot_v * n_dot_v * (1.0 - a2) + a2);
    let gl = n_dot_v * sqrt(n_dot_l * n_dot_l * (1.0 - a2) + a2);
    return 0.5 / max(gv + gl, 1e-6);
}

fn env_brdf(roughness: f32, n_dot_v: f32) -> vec2f {
    let c0 = vec4f(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4f(1.0, 0.0425, 1.04, -0.04);
    let r = c0 * roughness + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * n_dot_v)) * r.x + r.y;
    return vec2f(a004 * -1.04 + r.z, a004 * 1.04 + r.w);
}

// The visibility of a point in shadow map `index`: 1 when lit, 1 - strength in full shadow.
// It is the average of a 4x4 grid of bilinear comparisons.
fn shadow_visibility(index: i32, position: vec3f, normal: vec3f, n_dot_l: f32) -> f32 {
    let s = &frame.shadows[index];
    let params = s.params;
    let offset = normal * params.x * (1.0 + 2.0 * (1.0 - clamp(n_dot_l, 0.0, 1.0)));
    let clip = s.view_proj * vec4f(position + offset, 1.0);
    let p = clip.xyz / clip.w;
    let uv = vec2f(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if (uv.x <= 0.0 || uv.x >= 1.0 || uv.y <= 0.0 || uv.y >= 1.0 || p.z > 1.0) {
        return 1.0;
    }
    let params2 = s.params2;
    let z = p.z - params2.x * params.y;
    let texel = 1.0 / params2.y;
    let r = params.z;
    var lit = 0.0;
    for (var j = 0; j < 4; j++) {
        for (var i = 0; i < 4; i++) {
            let d = vec2f(f32(i) - 1.5, f32(j) - 1.5) / 1.5 * r * texel;
            lit += textureSampleCompareLevel(shadow_maps, shadow_sampler, uv + d, index, z);
        }
    }
    lit /= 16.0;
    return 1.0 - params.w * (1.0 - lit);
}

// The geometry pass.

// `uv` with the repeat and the offset of map `index` (see `Draw.uv_transforms`).
fn map_uv(uv: vec2f, index: i32) -> vec2f {
    let t = draw.uv_transforms[index];
    return uv * t.xy + t.zw;
}

struct VertexIn {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) color: vec4f,
}

// One copy of the mesh: the identity for a mesh without copies.
struct InstanceIn {
    @location(4) model_0: vec4f,
    @location(5) model_1: vec4f,
    @location(6) model_2: vec4f,
    @location(7) model_3: vec4f,
    @location(8) normal_0: vec4f,
    @location(9) normal_1: vec4f,
    @location(10) normal_2: vec4f,
    @location(11) color: vec4f,
}

struct VertexOut {
    @builtin(position) clip: vec4f,
    // The position relative to the camera. Its steps between pixels keep their precision
    // in close views and far from the origin.
    @location(0) @interpolate(perspective, centroid) rel: vec3f,
    @location(1) @interpolate(perspective, centroid) normal: vec3f,
    @location(2) @interpolate(perspective, centroid) uv: vec2f,
    @location(3) @interpolate(perspective, centroid) color: vec4f,
    @location(4) @interpolate(perspective, centroid) view_w: f32,
}

struct FragmentOut {
    @location(0) color: vec4f,
    // 1 / view distance, for the depth of field
    @location(1) key: vec4f,
}

@vertex
fn vs_main(in: VertexIn, copy: InstanceIn) -> VertexOut {
    var out: VertexOut;
    let model = mat4x4f(copy.model_0, copy.model_1, copy.model_2, copy.model_3);
    let normal = mat3x3f(copy.normal_0.xyz, copy.normal_1.xyz, copy.normal_2.xyz);
    let local = model * vec4f(in.position, 1.0);
    // The translation of the mesh minus the camera position is exact. A small part far
    // from the origin keeps its shape.
    let m = draw.model;
    let rel = mat3x3f(m[0].xyz, m[1].xyz, m[2].xyz) * local.xyz + (m[3].xyz - frame.camera.xyz) * local.w;
    out.clip = frame.view_proj_rel * vec4f(rel, 1.0);
    out.rel = rel;
    out.normal = (draw.normal_matrix * vec4f(normal * in.normal, 0.0)).xyz;
    out.uv = in.uv;
    out.color = in.color * copy.color;
    out.view_w = out.clip.w;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> FragmentOut {
    let world = in.rel + frame.camera.xyz;
    if (REFLECTION_PASS && dot(frame.clip_plane.xyz, world) + frame.clip_plane.w < 0.0) {
        discard;
    }
    // Derivatives and texture samples come first. They need uniform control flow.
    // In a close view the steps between pixels are micrometers, and the products of two
    // steps fall below the threshold of `normalize_or`. Only their directions matter, so
    // they are scaled to about 1.
    let step_x = dpdx(in.rel);
    let step_y = dpdy(in.rel);
    let step = 1.0 / max(largest(max(abs(step_x), abs(step_y))), 1e-30);
    let dx = step_x * step;
    let dy = step_y * step;
    // How much the smooth normal turns across the pixel, for the specular antialiasing.
    let n_smooth = normalize_or(in.normal, vec3f(0.0, 1.0, 0.0));
    let dnx = dpdx(n_smooth);
    let dny = dpdy(n_smooth);
    let normal_variation = dot(dnx, dnx) + dot(dny, dny);
    var texel = vec4f(1.0);
    var emissive_texel = vec4f(1.0);
    var mr_texel = vec4f(1.0);
    var ao_texel = vec4f(1.0);
    if (TEXTURED) {
        texel = textureSample(map_tex, tex_sampler, map_uv(in.uv, 0));
        emissive_texel = textureSample(emissive_tex, tex_sampler, map_uv(in.uv, 1));
        mr_texel = textureSample(mr_tex, tex_sampler, map_uv(in.uv, 2));
        ao_texel = textureSample(ao_tex, tex_sampler, map_uv(in.uv, 4));
    }
    // The normal map, and the directions of +U and +V on the triangle (`uv_directions` in
    // `shade.rs`).
    var normal_texel = vec4f(0.5, 0.5, 1.0, 1.0);
    var dpdu = vec3f(0.0);
    var dpdv = vec3f(0.0);
    if (NORMAL_MAP) {
        let uv = map_uv(in.uv, 3);
        normal_texel = textureSample(normal_tex, tex_sampler, uv);
        let uv_x = dpdx(uv);
        let uv_y = dpdy(uv);
        let uv_step = 1.0 / max(largest(vec3f(max(abs(uv_x), abs(uv_y)), 0.0)), 1e-30);
        let duvx = uv_x * uv_step;
        let duvy = uv_y * uv_step;
        let side = sign(duvx.x * duvy.y - duvy.x * duvx.y);
        dpdu = (dx * duvy.y - dy * duvx.y) * side;
        dpdv = (dy * duvx.x - dx * duvy.x) * side;
    }

    // The material comes next, so that the texels are not live during the lighting.
    var base = draw.color.rgb;
    var alpha = draw.color.a;
    if (draw.attrs.y > 0.5) {
        base *= in.color.rgb;
        alpha *= in.color.a;
    }
    if (draw.attrs.x > 0.5 && draw.maps.x > 0.5) {
        base *= texel.rgb;
        alpha *= texel.a;
    }
    var emissive = draw.emissive.rgb;
    if (draw.attrs.x > 0.5 && draw.maps.y > 0.5) {
        emissive *= emissive_texel.rgb;
    }
    var metal_rough = vec2f(1.0);
    if (draw.attrs.x > 0.5 && draw.maps.z > 0.5) {
        metal_rough = mr_texel.bg;
    }
    var occlusion = 1.0;
    if (draw.attrs.x > 0.5 && draw.detail.z > 0.5) {
        occlusion = ao_texel.r;
    }

    var v: vec3f;
    let to_camera = -in.rel;
    let distance = length(to_camera);
    if (frame.camera.w > 0.5) {
        v = -frame.forward.xyz;
    } else {
        v = to_camera / max(distance, 1e-6);
    }
    var face = normalize_or(cross(dx, dy), n_smooth);
    if (dot(face, v) < 0.0) {
        face = -face;
    }
    // The shadow maps come next. Each map needs sixteen samples and many registers. Few
    // other values are live at this point.
    var shadows = vec2f(1.0);
    if (SHADING == 0u && draw.maps.w > 0.5) {
        shadows = shadow_factors(world, face);
    }
    var n = n_smooth;
    if (draw.flags.x > 0.5) {
        n = face;
    }
    if (NORMAL_MAP && draw.attrs.x > 0.5) {
        n = bend(n, dpdu, dpdv, normal_texel, draw.detail.y);
    }
    // A back face, or a smooth normal that points away from the camera near a silhouette,
    // turns around.
    if (dot(n, face) < 0.0) {
        n = -n;
    }

    let n_dot_v = max(dot(n, v), 1e-4);

    var out: FragmentOut;
    out.key = vec4f(1.0 / max(in.view_w, 1e-4), 0.0, 0.0, 1.0);
    var rgb = vec3f(0.0);
    if (SHADING == 1u) {
        rgb = base + emissive;
    } else if (SHADING == 3u) {
        rgb = n * 0.5 + 0.5;
    } else if (SHADING == 2u) {
        var shadow = 1.0;
        for (var i = 0; i < light_count(); i++) {
            let light = &frame.lights[i];
            let index = light.params.z;
            if (i32(light.color.w) == 0 && index >= 0.0) {
                shadow *= shadow_visibility(i32(index), world, face, dot(face, -light.direction.xyz));
            }
        }
        out.color = vec4f(0.0, 0.0, 0.0, (1.0 - shadow) * draw.color.a);
        return out;
    } else {
        rgb = standard(in.rel, in.clip.xy, base, n, face, v, n_dot_v, metal_rough, occlusion, shadows, normal_variation) + emissive;
    }

    if (FOG && draw.flags.z > 0.5 && frame.fog.w > 0.5) {
        var f = 0.0;
        if (frame.fog.w < 1.5) {
            f = clamp((distance - frame.fog_params.x) / (frame.fog_params.y - frame.fog_params.x), 0.0, 1.0);
        } else {
            f = 1.0 - exp(-pow(frame.fog_params.z * distance, 2.0));
        }
        if (draw.flags.y > 0.5) {
            rgb *= 1.0 - f;
        } else {
            rgb = mix(rgb, frame.fog.rgb, f);
        }
    }
    // The HDR target is a 16 bit float. A sharp sun glint can overflow to infinity.
    rgb = clamp(rgb, vec3f(0.0), vec3f(30000.0));
    // An opaque surface covers its pixels whatever the alpha of its texture.
    if (draw.flags.w < 0.5) {
        alpha = 1.0;
    }
    if (draw.flags.y > 0.5) {
        out.color = vec4f(rgb * alpha, 0.0);
    } else {
        out.color = vec4f(rgb * alpha, alpha);
    }
    return out;
}

// The normal `n` bent by the normal map `texel` (`bend` in `shade.rs`).
fn bend(n: vec3f, dpdu: vec3f, dpdv: vec3f, texel: vec4f, scale: f32) -> vec3f {
    let m = texel.rgb * 2.0 - 1.0;
    let t = normalize_or(dpdu - n * dot(n, dpdu), vec3f(0.0));
    let b = normalize_or(dpdv - n * dot(n, dpdv), vec3f(0.0));
    return normalize_or(t * (m.x * scale) - b * (m.y * scale) + n * m.z, n);
}

// The visibility of a point in each of the two shadow maps, see `shadow_visibility`.
fn shadow_factors(position: vec3f, face: vec3f) -> vec2f {
    var shadows = vec2f(1.0);
    for (var i = 0; i < light_count(); i++) {
        let light = &frame.lights[i];
        let index = i32(light.params.z);
        if (i32(light.color.w) != 0 || index < 0 || index > 1) {
            continue;
        }
        let fl = dot(face, -light.direction.xyz);
        if (fl > 0.0) {
            shadows[index] = shadow_visibility(index, position, face, fl);
        }
    }
    return shadows;
}

// The light of a standard (PBR) surface at `rel`, the position relative to the camera.
// `metal_rough` scales the metalness and the roughness. `shadows` comes from
// `shadow_factors`. `normal_variation` tells how much the normal turns across the pixel.
fn standard(rel: vec3f, screen: vec2f, base: vec3f, n: vec3f, face: vec3f, v: vec3f, n_dot_v: f32, metal_rough: vec2f, occlusion: f32, shadows: vec2f, normal_variation: f32) -> vec3f {
    var metalness = draw.pbr.x * metal_rough.x;
    var roughness = draw.pbr.y * metal_rough.y;
    roughness = clamp(roughness, 0.045, 1.0);
    metalness = clamp(metalness, 0.0, 1.0);
    var alpha = roughness * roughness;
    let clearcoat = clamp(draw.pbr.z, 0.0, 1.0);
    var cc_roughness = draw.pbr.w;
    var cc_alpha = pow(clamp(cc_roughness, 0.045, 1.0), 2.0);
    // Specular antialiasing (Kaplanyan 2016, with the constants of Filament). A normal that
    // turns within the pixel makes the highlight wider, so that it does not sparkle.
    let kernel = min(0.3 * normal_variation, 0.04);
    if (kernel > 0.0) {
        alpha = sqrt(min(alpha * alpha + kernel, 1.0));
        roughness = sqrt(alpha);
        cc_alpha = sqrt(min(cc_alpha * cc_alpha + kernel, 1.0));
        cc_roughness = sqrt(cc_alpha);
    }
    let f0 = mix(vec3f(0.04), base, metalness);
    let diffuse = base * (1.0 - metalness);

    // The light from the environment comes first. `face` is not used after it, so the light
    // loop uses few enough registers for the full SIMD width.
    var irradiance = frame.ambient.rgb / PI;
    if (frame.ambient.w > 0.5) {
        irradiance += mix(frame.hemi_ground.rgb, frame.hemi_sky.rgb, n.y * 0.5 + 0.5) / PI;
    }
    let has_env = env_kind() != 0;
    let env_intensity = draw.emissive.w;
    if (has_env) {
        irradiance += env_irradiance(n) * env_intensity;
    }
    var indirect = diffuse * irradiance;
    let brdf = env_brdf(roughness, n_dot_v);
    if (has_env) {
        var r = reflect(-v, n);
        r = normalize_or(mix(r, n, alpha), n);
        let horizon = pow(clamp(1.0 + dot(r, face), 0.0, 1.0), 2.0);
        let env = env_radiance(r, roughness) * env_intensity;
        var radiance = env * horizon;
        if (REFLECTION) {
            // The reflection of the scene replaces the environment where it covers the pixel.
            let p = planar_reflection(screen, roughness);
            radiance = mix(radiance, (p.rgb + env * (1.0 - p.a)) * horizon, draw.attrs.z);
        }
        indirect += radiance * (f0 * brdf.x + vec3f(brdf.y));
        if (CLEARCOAT && clearcoat > 0.0) {
            let rc = reflect(-v, n);
            let cb = env_brdf(cc_roughness, n_dot_v);
            let fc = (0.04 * cb.x + cb.y) * clearcoat;
            var coat = env_radiance(rc, cc_roughness) * env_intensity;
            if (REFLECTION) {
                let p = planar_reflection(screen, cc_roughness);
                coat = mix(coat, p.rgb + coat * (1.0 - p.a), draw.attrs.z);
            }
            indirect = indirect * (1.0 - fc) + coat * (horizon * fc);
        }
    } else {
        indirect += irradiance * (f0 * brdf.x + vec3f(brdf.y)) * 0.5;
    }
    indirect *= occlusion;

    var direct = vec3f(0.0);
    for (var i = 0; i < light_count(); i++) {
        let light = &frame.lights[i];
        let color = light.color;
        let kind = i32(color.w);
        var l = vec3f(0.0);
        var radiance = color.rgb;
        if (kind == 0) {
            l = -light.direction.xyz;
            let index = i32(light.params.z);
            if (index >= 0 && index <= 1) {
                radiance *= shadows[index];
            }
        } else {
            let lp = light.position;
            let ld = light.direction;
            // Both positions relative to the camera: a light close to a small part far
            // from the origin keeps its direction.
            let d = (lp.xyz - frame.camera.xyz) - rel;
            let dist = length(d);
            l = d / max(dist, 1e-6);
            radiance *= attenuation(dist, lp.w, ld.w);
            if (kind == 2) {
                let cone = light.params;
                radiance *= smooth_step(cone.x, cone.y, dot(-l, ld.xyz));
            }
        }
        let n_dot_l = dot(n, l);
        if (n_dot_l <= 0.0 || max(radiance.r, max(radiance.g, radiance.b)) <= 0.0) {
            continue;
        }
        let h = normalize_or(l + v, n);
        let n_dot_h = max(dot(n, h), 0.0);
        let v_dot_h = max(dot(v, h), 0.0);
        let fresnel = f0 + (vec3f(1.0) - f0) * pow(1.0 - v_dot_h, 5.0);
        let specular = fresnel * (ggx(n_dot_h, alpha) * smith_visibility(n_dot_l, n_dot_v, alpha));
        var lit = (diffuse * (vec3f(1.0) - fresnel) / PI + specular) * radiance * n_dot_l;
        if (CLEARCOAT && clearcoat > 0.0) {
            let fc = (0.04 + 0.96 * pow(1.0 - v_dot_h, 5.0)) * clearcoat;
            let coat = ggx(n_dot_h, cc_alpha) * smith_visibility(n_dot_l, n_dot_v, cc_alpha) * fc;
            lit = lit * (1.0 - fc) + radiance * (coat * n_dot_l);
        }
        direct += lit;
    }
    return direct + indirect;
}

// The background.

struct FullscreenOut {
    @builtin(position) clip: vec4f,
    @location(0) ndc: vec2f,
}

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> FullscreenOut {
    let uv = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    var out: FullscreenOut;
    out.ndc = uv * 2.0 - 1.0;
    out.clip = vec4f(out.ndc, 0.0, 1.0);
    return out;
}

@fragment
fn fs_background(in: FullscreenOut) -> FragmentOut {
    var out: FragmentOut;
    out.key = vec4f(0.0);
    let kind = i32(frame.background.x);
    if (kind == 1) {
        out.color = vec4f(frame.background_color.rgb, 1.0);
    } else {
        let near = frame.inv_view_proj * vec4f(in.ndc, 1.0, 1.0);
        let far = frame.inv_view_proj * vec4f(in.ndc, 0.0, 1.0);
        let dir = normalize_or(far.xyz / far.w - near.xyz / near.w, vec3f(0.0, 0.0, -1.0));
        out.color = vec4f(env_radiance(dir, frame.background.y), 1.0);
    }
    return out;
}

// The particles.

struct ParticleIn {
    // xyz, size
    @location(0) position: vec4f,
    // rgb times opacity, opacity
    @location(1) color: vec4f,
}

struct ParticleOut {
    @builtin(position) clip: vec4f,
    @location(0) @interpolate(flat) center: vec2f,
    @location(1) @interpolate(flat) radius: f32,
    // rgb times opacity, the coverage of a blended particle
    @location(2) @interpolate(flat) color: vec4f,
}

// A particle is a round dot, drawn as a quad around its center.
@vertex
fn vs_particle(in: ParticleIn, @builtin(vertex_index) index: u32) -> ParticleOut {
    var out: ParticleOut;
    let c = frame.view_proj * vec4f(in.position.xyz, 1.0);
    // A particle behind the near plane moves out of the view.
    if (c.w <= frame.viewport.w) {
        out.clip = vec4f(2.0, 2.0, 2.0, 1.0);
        return out;
    }
    let ndc = c.xy / c.w;
    let center = vec2f((ndc.x * 0.5 + 0.5) * frame.viewport.x, (0.5 - ndc.y * 0.5) * frame.viewport.y);
    var radius = in.position.w * frame.viewport.z;
    if (frame.camera.w < 0.5) {
        radius /= c.w;
    }
    // A particle smaller than 0.6 pixels is drawn at 0.6 pixels with less energy.
    let drawn = max(radius, 0.6);
    let energy = (radius / drawn) * (radius / drawn);
    out.color = in.color * energy;
    out.center = center;
    out.radius = drawn;
    let corners = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));
    let p = center + corners[index] * (drawn + 1.0);
    out.clip = vec4f(p.x / frame.viewport.x * 2.0 - 1.0, 1.0 - p.y / frame.viewport.y * 2.0, c.z / c.w, 1.0);
    return out;
}

fn particle_alpha(in: ParticleOut) -> f32 {
    let d = distance(in.clip.xy, in.center);
    let r = in.radius;
    let falloff = 1.0 - clamp((d - r * 0.3) / (r * 0.7 + 0.5), 0.0, 1.0);
    return falloff * falloff;
}

@fragment
fn fs_particle_additive(in: ParticleOut) -> FragmentOut {
    var out: FragmentOut;
    out.key = vec4f(0.0);
    out.color = vec4f(in.color.rgb * particle_alpha(in), 0.0);
    return out;
}

@fragment
fn fs_particle_normal(in: ParticleOut) -> FragmentOut {
    var out: FragmentOut;
    out.key = vec4f(0.0);
    let a = particle_alpha(in);
    out.color = vec4f(in.color.rgb * a, in.color.a * a);
    return out;
}
