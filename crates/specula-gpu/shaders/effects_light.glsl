// Shared by the effect shaders (particle.*, decal.*): the scene's display transform, the sun's
// shadow lookup and point lights, the same formulas as the mesh and sprite pipelines (see
// shaders/sprite.frag). Included after the stage's push block (`pc.frame`) and its FEATURES
// constant; not compiled on its own.

#ifndef NO_DISPLAY
// Integer hash (PCG), for dithering.
uint pcg(uint v) {
    uint state = v * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}
float hash_u(uint a, uint b) { return float(pcg(a + pcg(b)) >> 8) / 16777216.0; }

vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

// Linear scene light -> the value written: the same as the mesh pipeline's `display`.
vec3 display(vec3 c) {
    c *= pc.frame.ambient.w;
    if (HDR) {
        return clamp(c, vec3(0.0), vec3(60000.0));
    }
    if (pc.frame.sky.w > 0.5) {
        c = aces(c);
        float n = hash_u(uint(gl_FragCoord.x), uint(gl_FragCoord.y)) - 0.5;
        c += n * 2.2 * pow(max(c, vec3(1e-4)), vec3(0.545)) / 255.0;
    }
    return max(c, vec3(0.0));
}
#endif

// Diffuse light from the frame's point lights at `p` (normal `n`; a zero normal lights from
// every side, for volumes such as smoke).
vec3 point_lights(vec3 p, vec3 n) {
    vec3 sum = vec3(0.0);
    int count = int(pc.frame.viewport.z);
    for (int i = 0; i < count; i++) {
        vec3 to_light = pc.frame.lights[i].pos_range.xyz - p;
        float d2 = dot(to_light, to_light);
        float d = sqrt(d2);
        float x = d / pc.frame.lights[i].pos_range.w;
        float window = clamp(1.0 - x * x * x * x, 0.0, 1.0);
        vec3 radiance = pc.frame.lights[i].color.rgb * window * window / (d2 + 0.0025);
        float ndl = dot(n, n) > 0.0 ? max(dot(n, to_light / max(d, 1e-5)), 0.0) : 0.6;
        sum += radiance * ndl;
    }
    return sum;
}

#ifdef SUN_SHADOW
const vec2 POISSON[12] = vec2[](
    vec2(-0.326, -0.406), vec2(-0.840, -0.074), vec2(-0.696, 0.457), vec2(-0.203, 0.621),
    vec2(0.962, -0.195), vec2(0.473, -0.480), vec2(0.519, 0.767), vec2(0.185, -0.893),
    vec2(0.507, 0.064), vec2(0.896, 0.412), vec2(-0.322, -0.933), vec2(-0.792, -0.598));

// The mesh pipeline's sun shadow lookup (see shaders/mesh.frag).
float sun_shadow(vec3 p, vec3 n) {
    float depth = dot(pc.frame.shadow_axis.xyz, p) + pc.frame.shadow_axis.w;
    float fade = smoothstep(pc.frame.shadow_fade.x, pc.frame.shadow_fade.y, depth);
    if (fade >= 1.0) {
        return 1.0;
    }
    float size = float(textureSize(shadow_map, 0).x);
    int count = int(pc.frame.shadow_params.x);
    for (int k = 0; k < count; k++) {
        float texel = pc.frame.shadow_texel[k];
        float radius = max(pc.frame.shadow_params.y * pc.frame.shadow_texel.x / texel, 1.0);
        vec4 s = pc.frame.shadow_mats[k] * vec4(p + n * (texel * pc.frame.shadow_params.z), 1.0);
        vec2 uv = s.xy * 0.5 + 0.5;
        float margin = (radius + 1.5) / size;
        if (all(greaterThan(uv, vec2(margin))) && all(lessThan(uv, vec2(1.0 - margin))) && s.z <= 1.0) {
            float lit = 0.0;
            for (int i = 0; i < 12; i++) {
                lit += texture(shadow_map, vec4(uv + POISSON[i] * (radius / size), float(k), s.z));
            }
            return mix(lit / 12.0, 1.0, fade);
        }
    }
    return 1.0;
}
#endif
