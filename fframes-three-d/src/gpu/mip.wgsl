// The mipmap levels of a material texture from an image on the GPU (an svgr image or a 3D
// render). They follow `levels_of_premultiplied` and `downsample` in `texture.rs`. The
// target is an sRGB level, so the shaders return linear colors.

@group(0) @binding(0) var src: texture_2d<f32>;

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let uv = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

// Level 0: the premultiplied sRGB texels of the image with straight alpha, rounded like
// on the CPU.
@fragment
fn fs_level0(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let texel = round(textureLoad(src, vec2i(position.xy), 0) * 255.0);
    var rgb = texel.rgb;
    if texel.a > 0.0 && texel.a < 255.0 {
        rgb = min(floor(rgb * 255.0 / texel.a), vec3f(255.0));
    }
    let c = rgb / 255.0;
    return vec4f(to_linear(c.r), to_linear(c.g), to_linear(c.b), texel.a / 255.0);
}

// The next level: each texel is the mean of four texels of the level before, in linear
// light. The level before is an sRGB view, so its texels load as linear.
@fragment
fn fs_down(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let last = vec2i(textureDimensions(src)) - 1;
    let base = vec2i(position.xy) * 2;
    var sum = vec4f(0.0);
    for (var i = 0; i < 4; i++) {
        sum += textureLoad(src, min(base + vec2i(i & 1, i >> 1), last), 0);
    }
    return sum / 4.0;
}
