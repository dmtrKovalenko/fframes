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

@vertex
fn vs_shadow(@location(0) position: vec3f) -> @builtin(position) vec4f {
    return shadow_pass.view_proj * (draw.model * vec4f(position, 1.0));
}
