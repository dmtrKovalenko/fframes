// The depth pass into a shadow map.

struct ShadowPass {
    view_proj: mat4x4f,
}

// The start of `Draw` in `scene.wgsl`.
struct Draw {
    model: mat4x4f,
}

@group(0) @binding(0) var<uniform> shadow_pass: ShadowPass;
@group(1) @binding(0) var<uniform> draw: Draw;

// The model matrix of one copy of the mesh: the identity for a mesh without copies.
struct InstanceIn {
    @location(1) model_0: vec4f,
    @location(2) model_1: vec4f,
    @location(3) model_2: vec4f,
    @location(4) model_3: vec4f,
}

struct ShadowOut {
    @builtin(position) clip: vec4f,
    @location(0) depth: f32,
}

// The light camera is orthographic: `w` is 1 and the depth is linear across a triangle.
// The rasterizer would clip a caster beyond the near plane, but a caster between the light
// and the map still throws its shadow. The vertex sits in the middle of the depth range,
// and the fragment writes the depth clamped to the near plane, as the CPU rasterizer does.
@vertex
fn vs_shadow(@location(0) position: vec3f, copy: InstanceIn) -> ShadowOut {
    let model = mat4x4f(copy.model_0, copy.model_1, copy.model_2, copy.model_3);
    let clip = shadow_pass.view_proj * (draw.model * (model * vec4f(position, 1.0)));
    var out: ShadowOut;
    out.clip = vec4f(clip.xy, 0.5 * clip.w, clip.w);
    out.depth = clip.z / clip.w;
    return out;
}

@fragment
fn fs_shadow(in: ShadowOut) -> @builtin(frag_depth) f32 {
    return clamp(in.depth, 0.0, 1.0);
}
