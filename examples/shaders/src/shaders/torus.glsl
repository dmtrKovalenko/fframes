// Shadertoy-style raymarcher: a spinning torus around a breathing sphere.
#define STEPS 80
#define FAR 10.0
#define TAU 6.2831853

mat2 rot(float a) {
    float c = cos(a);
    float s = sin(a);
    return mat2(c, -s, s, c);
}

float sdTorus(vec3 p, vec2 t) {
    vec2 q = vec2(length(p.xz) - t.x, p.y);
    return length(q) - t.y;
}

float map(vec3 p) {
    p.xy = rot(iTime * 0.7) * p.xy;
    p.yz = rot(iTime * 0.5) * p.yz;
    float torus = sdTorus(p, vec2(1.0, 0.35));
    float sphere = length(p) - 0.55 - 0.08 * sin(iTime * 3.0);
    return min(torus, sphere);
}

vec3 calcNormal(vec3 p) {
    vec2 e = vec2(0.001, 0.0);
    return normalize(vec3(
        map(p + e.xyy) - map(p - e.xyy),
        map(p + e.yxy) - map(p - e.yxy),
        map(p + e.yyx) - map(p - e.yyx)
    ));
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = (fragCoord - 0.5 * iResolution.xy) / iResolution.y;
    vec3 ro = vec3(0.0, 0.0, -5.2);
    vec3 rd = normalize(vec3(uv, 1.4));

    float d = 0.0;
    bool hit = false;
    for (int i = 0; i < STEPS; i++) {
        float h = map(ro + rd * d);
        if (h < 0.001) {
            hit = true;
            break;
        }
        d += h;
        if (d > FAR) {
            break;
        }
    }

    vec3 col = mix(vec3(0.05, 0.04, 0.10), vec3(0.14, 0.08, 0.26), uv.y + 0.5);
    if (hit) {
        vec3 p = ro + rd * d;
        vec3 n = calcNormal(p);
        vec3 l = normalize(vec3(0.6, 0.8, -0.5));
        float diffuse = max(dot(n, l), 0.0);
        float specular = pow(max(dot(reflect(-l, n), -rd), 0.0), 32.0);
        float fresnel = pow(1.0 - max(dot(n, -rd), 0.0), 3.0);
        vec3 albedo = 0.5 + 0.5 * cos(TAU * (vec3(0.0, 0.33, 0.67) + p.y * 0.3 + iTime * 0.1));
        col = albedo * (0.15 + 0.85 * diffuse) + specular + fresnel * vec3(0.5, 0.4, 1.0);
    }

    fragColor = vec4(pow(col, vec3(0.4545)), 1.0);
}
