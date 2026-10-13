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
    // xyz, 1 for an orthographic camera
    camera: vec4f,
    // xyz, angle of one pixel
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
    // kind (0 none, 1 gradient, 2 studio, 3 map), intensity, rotation, softbox count
    env: vec4f,
    env_sky: vec4f,
    env_horizon: vec4f,
    env_ground: vec4f,
    // kind (0 transparent, 1 color, 2 environment, 3 gradient), blur, 0, 0
    background: vec4f,
    bg_top: vec4f,
    bg_bottom: vec4f,
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
    // rgb, power
    rim: vec4f,
    // rgb, 1 with a grid
    grid_color: vec4f,
    // cell, line width, fade distance, 0
    grid: vec4f,
    // flat shading, additive, fog, 0
    flags: vec4f,
    // has map, has emissive map, has metal rough map, receives shadows
    maps: vec4f,
    // has uv, has color, reflection, 0
    attrs: vec4f,
    // uv repeat, uv offset
    uv_transform: vec4f,
}

// The pipeline constants of a material (`Variant` in `mod.rs`). A material compiles only the
// paths that it uses. This keeps the shader small enough to run at full SIMD width. The
// background and particle pipelines use the default values.
override SHADING: u32 = 0u; // 0 standard, 1 unlit, 2 shadow catcher, 3 normals
override ENVIRONMENT: u32 = 255u; // 0 none, 1 gradient, 2 studio, 3 map, 255 from the frame
override CLEARCOAT: bool = true;
override TEXTURED: bool = true;
override GRID: bool = true;
override FOG: bool = true;
override RIM: bool = true;
override LIGHTS: u32 = 255u; // the light count, 255 from the frame
override REFLECTION: bool = false; // the material shows the reflection of the scene
override REFLECTION_PASS: bool = false; // draws the reflection: nothing behind the reflector

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

// The smoothstep of `render/shade.rs`. The formula is explicit, so that both renderers
// compute the same value.
fn smooth_step(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
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
    if (kind == 1) {
        return gradient(dir_in.y, roughness) * intensity;
    }
    if (kind == 2 || kind == 3) {
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
    if (kind == 1) {
        return gradient(n_in.y, 1.0) * intensity;
    }
    let n = rotate_y(n_in, -frame.env.z);
    if (kind == 2) {
        var color = gradient(n.y, 1.0);
        let count = i32(frame.env.w);
        for (var i = 0; i < count; i++) {
            let b = &frame.softboxes[i];
            let direction = b.direction;
            color += b.color.rgb * (direction.w * max(dot(n, direction.xyz), 0.0) / PI);
        }
        return color * intensity;
    }
    if (kind == 3) {
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

// How much of the pixel the grid lines cover. The grid lies in the axis plane that is most
// perpendicular to the normal `n`.
fn grid_coverage(p: vec3f, n: vec3f, distance: f32, n_dot_v: f32) -> f32 {
    let a = abs(n);
    var u = p.x;
    var v = p.y;
    if (a.y >= a.x && a.y >= a.z) {
        v = p.z;
    } else if (a.x >= a.z) {
        u = p.y;
        v = p.z;
    }
    // The size of the pixel on the surface.
    let footprint = max(distance * frame.forward.w / max(n_dot_v, 0.2), 1e-5);
    let lines_u = line_coverage(u, footprint);
    let lines_v = line_coverage(v, footprint);
    return lines_u + lines_v - lines_u * lines_v;
}

// How much of the pixel the grid lines along one axis cover at coordinate `c`.
fn line_coverage(c: f32, footprint: f32) -> f32 {
    let cell = draw.grid.x;
    let line_width = draw.grid.y;
    let f = abs(fract(abs(c / cell)) - 0.5);
    let d = (0.5 - f) * cell;
    let half_width = line_width * 0.5;
    let coverage = clamp((half_width + footprint * 0.5 - d) / footprint, 0.0, 1.0);
    return coverage * min(line_width / footprint, 1.0);
}

// The geometry pass.

struct VertexIn {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) color: vec4f,
}

struct VertexOut {
    @builtin(position) clip: vec4f,
    @location(0) @interpolate(perspective, centroid) world: vec3f,
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
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    let world = draw.model * vec4f(in.position, 1.0);
    out.clip = frame.view_proj * world;
    out.world = world.xyz;
    out.normal = (draw.normal_matrix * vec4f(in.normal, 0.0)).xyz;
    out.uv = in.uv;
    out.color = in.color;
    out.view_w = out.clip.w;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> FragmentOut {
    if (REFLECTION_PASS && dot(frame.clip_plane.xyz, in.world) + frame.clip_plane.w < 0.0) {
        discard;
    }
    // Derivatives and texture samples come first. They need uniform control flow.
    let dx = dpdx(in.world);
    let dy = dpdy(in.world);
    // How much the smooth normal turns across the pixel, for the specular antialiasing.
    let n_smooth = normalize_or(in.normal, vec3f(0.0, 1.0, 0.0));
    let dnx = dpdx(n_smooth);
    let dny = dpdy(n_smooth);
    let normal_variation = dot(dnx, dnx) + dot(dny, dny);
    let uv = in.uv * draw.uv_transform.xy + draw.uv_transform.zw;
    var texel = vec4f(1.0);
    var emissive_texel = vec4f(1.0);
    var mr_texel = vec4f(1.0);
    if (TEXTURED) {
        texel = textureSample(map_tex, tex_sampler, uv);
        emissive_texel = textureSample(emissive_tex, tex_sampler, uv);
        mr_texel = textureSample(mr_tex, tex_sampler, uv);
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

    var v: vec3f;
    let to_camera = frame.camera.xyz - in.world;
    let distance = length(to_camera);
    if (frame.camera.w > 0.5) {
        v = -frame.forward.xyz;
    } else {
        v = to_camera / max(distance, 1e-6);
    }
    var face = normalize_or(cross(dx, dy), vec3f(0.0, 1.0, 0.0));
    if (dot(face, v) < 0.0) {
        face = -face;
    }
    // The shadow maps come next. Each map needs sixteen samples and many registers. Few
    // other values are live at this point.
    var shadows = vec2f(1.0);
    if (SHADING == 0u && draw.maps.w > 0.5) {
        shadows = shadow_factors(in.world, face);
    }
    var n = n_smooth;
    if (draw.flags.x > 0.5) {
        n = face;
    }
    if (dot(n, face) < 0.0) {
        n = -n;
    }

    let n_dot_v = max(dot(n, v), 1e-4);
    if (RIM) {
        emissive += draw.rim.rgb * pow(1.0 - n_dot_v, draw.rim.w);
    }
    if (GRID && draw.grid_color.w > 0.5) {
        emissive += draw.grid_color.rgb * grid_coverage(in.world, face, distance, n_dot_v) * (1.0 - smooth_step(draw.grid.z * 0.5, draw.grid.z, distance));
    }

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
                shadow *= shadow_visibility(i32(index), in.world, face, dot(face, -light.direction.xyz));
            }
        }
        out.color = vec4f(0.0, 0.0, 0.0, (1.0 - shadow) * draw.color.a);
        return out;
    } else {
        rgb = standard(in.world, in.clip.xy, base, n, face, v, n_dot_v, metal_rough, shadows, normal_variation) + emissive;
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
    if (draw.flags.y > 0.5) {
        out.color = vec4f(rgb * alpha, 0.0);
    } else {
        out.color = vec4f(rgb * alpha, alpha);
    }
    return out;
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

// The light of a standard (PBR) surface. `metal_rough` scales the metalness and the
// roughness. `shadows` comes from `shadow_factors`. `normal_variation` tells how much the
// normal turns across the pixel.
fn standard(position: vec3f, screen: vec2f, base: vec3f, n: vec3f, face: vec3f, v: vec3f, n_dot_v: f32, metal_rough: vec2f, shadows: vec2f, normal_variation: f32) -> vec3f {
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
            let d = lp.xyz - position;
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
        out.color = vec4f(frame.bg_top.rgb, 1.0);
    } else if (kind == 3) {
        let t = 0.5 - in.ndc.y * 0.5;
        out.color = vec4f(mix(frame.bg_top.rgb, frame.bg_bottom.rgb, t), 1.0);
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
    @location(0) start: vec4f,
    // xyz of the tail end, 0
    @location(1) end: vec4f,
    // rgb times opacity, 0
    @location(2) color: vec4f,
}

struct ParticleOut {
    @builtin(position) clip: vec4f,
    @location(0) @interpolate(flat) p0: vec2f,
    @location(1) @interpolate(flat) p1: vec2f,
    @location(2) @interpolate(flat) radius: f32,
    @location(3) @interpolate(flat) color: vec3f,
}

fn to_screen(clip: vec4f) -> vec2f {
    let ndc = clip.xy / clip.w;
    return vec2f((ndc.x * 0.5 + 0.5) * frame.viewport.x, (0.5 - ndc.y * 0.5) * frame.viewport.y);
}

// A particle is a capsule from its position to the end of its tail, drawn as a quad.
@vertex
fn vs_particle(in: ParticleIn, @builtin(vertex_index) index: u32) -> ParticleOut {
    var out: ParticleOut;
    let c0 = frame.view_proj * vec4f(in.start.xyz, 1.0);
    let c1 = frame.view_proj * vec4f(in.end.xyz, 1.0);
    // A particle behind the near plane moves out of the view.
    if (c0.w <= frame.viewport.w) {
        out.clip = vec4f(2.0, 2.0, 2.0, 1.0);
        return out;
    }
    let s0 = to_screen(c0);
    var s1 = s0;
    var z = c0.z / c0.w;
    if (c1.w > frame.viewport.w) {
        s1 = to_screen(c1);
        z = max(z, c1.z / c1.w);
    }
    var radius = in.start.w * frame.viewport.z;
    if (frame.camera.w < 0.5) {
        radius /= c0.w;
    }
    // A particle smaller than 0.6 pixels is drawn at 0.6 pixels with less energy. A tail
    // spreads the energy over its length.
    let drawn = max(radius, 0.6);
    let energy = (radius / drawn) * (radius / drawn);
    let length_px = distance(s0, s1);
    let smear = (2.0 * drawn) / (2.0 * drawn + length_px);
    out.color = in.color.rgb * energy * smear;
    out.p0 = s0;
    out.p1 = s1;
    out.radius = drawn;
    var axis = vec2f(1.0, 0.0);
    if (length_px > 1e-3) {
        axis = (s1 - s0) / length_px;
    }
    let side = vec2f(-axis.y, axis.x);
    let pad = drawn + 1.0;
    let corners = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));
    let k = corners[index];
    var p = s0;
    if (k.x > 0.0) {
        p = s1;
    }
    p = p + axis * (k.x * pad) + side * (k.y * pad);
    let ndc = vec2f(p.x / frame.viewport.x * 2.0 - 1.0, 1.0 - p.y / frame.viewport.y * 2.0);
    out.clip = vec4f(ndc, z, 1.0);
    return out;
}

fn particle_alpha(in: ParticleOut) -> f32 {
    let c = in.clip.xy;
    let seg = in.p1 - in.p0;
    let t = clamp(dot(c - in.p0, seg) / max(dot(seg, seg), 1e-8), 0.0, 1.0);
    let d = distance(c, in.p0 + seg * t);
    let r = in.radius;
    let falloff = 1.0 - clamp((d - r * 0.3) / (r * 0.7 + 0.5), 0.0, 1.0);
    return falloff * falloff;
}

@fragment
fn fs_particle_additive(in: ParticleOut) -> FragmentOut {
    var out: FragmentOut;
    out.key = vec4f(0.0);
    out.color = vec4f(in.color * particle_alpha(in), 0.0);
    return out;
}

@fragment
fn fs_particle_normal(in: ParticleOut) -> FragmentOut {
    var out: FragmentOut;
    out.key = vec4f(0.0);
    let a = particle_alpha(in);
    out.color = vec4f(in.color * a, a);
    return out;
}
