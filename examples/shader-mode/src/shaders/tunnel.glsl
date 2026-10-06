// Original Shadertoy-style program. fframes supplies the built-in uniforms and
// translates mainImage to SkSL, including the bottom-left coordinate convention.
#define TAU 6.2831853
uniform float uProgress;
uniform float uVariant;
uniform float uBeat;

vec2 rotate(vec2 p, float a) {
    return vec2(cos(a) * p.x - sin(a) * p.y, sin(a) * p.x + cos(a) * p.y);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 p = (fragCoord - iResolution.xy * 0.5) / iResolution.y;
    p = rotate(p, 0.3 + iTime * 0.28);
    float angle = atan(p.y, p.x);
    float sector = TAU / 7.0;
    float polygon = cos(floor(0.5 + angle / sector) * sector - angle) * length(p);
    float depth = -log(max(0.004, polygon)) * 2.2 - iTime * 2.0;
    float bands = fract(depth);
    vec3 color = mix(vec3(0.28, 0.01, 0.32), vec3(1.0, 0.73, 0.13), pow(bands, 0.55));
    color *= 0.55 + 0.45 * smoothstep(0.02, 0.16, polygon);
    vec2 q = rotate(p, -iTime * 1.2);
    float twist = q.y + 0.18 * sin(q.x * 7.0 + iTime);
    float ribbon = 1.0 - smoothstep(0.07, 0.078, abs(twist));
    ribbon *= 1.0 - smoothstep(0.42, 0.45, abs(q.x));
    float perforation = smoothstep(0.12, 0.18, length(fract(q * 125.0) - 0.5));
    color = mix(color, vec3(0.055, 0.008, 0.065) + 0.15 * (1.0 - perforation), ribbon);
    fragColor = vec4(color, 1.0);
}
