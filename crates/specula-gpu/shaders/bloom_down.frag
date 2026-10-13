#version 460
// Bloom downsample: a 13-tap filter (Jimenez 2014) from the level above into this one.
// mode 1 = the first level, read from the HDR scene: each tap group is Karis-weighted (no
// fireflies from single bright pixels) and only light above the threshold passes.
// params: threshold, knee. Compile with shaders/build.sh.
#include "post.glsl"

vec3 tap(vec2 offset) {
    return texture(textures[pc.src], v_uv + offset * pc.texel).rgb;
}

float luma(vec3 c) {
    return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

// Group average, weighted down when bright (only on the first level).
vec3 group(vec3 a, vec3 b, vec3 c, vec3 d) {
    vec3 avg = 0.25 * (a + b + c + d);
    return pc.mode == 1u ? avg / (1.0 + luma(avg)) : avg;
}

void main() {
    vec3 a = tap(vec2(-2.0, -2.0)), b = tap(vec2(0.0, -2.0)), c = tap(vec2(2.0, -2.0));
    vec3 d = tap(vec2(-2.0, 0.0)), e = tap(vec2(0.0, 0.0)), f = tap(vec2(2.0, 0.0));
    vec3 g = tap(vec2(-2.0, 2.0)), h = tap(vec2(0.0, 2.0)), i = tap(vec2(2.0, 2.0));
    vec3 j = tap(vec2(-1.0, -1.0)), k = tap(vec2(1.0, -1.0)), l = tap(vec2(-1.0, 1.0)), m = tap(vec2(1.0, 1.0));
    vec3 sum = 0.5 * group(j, k, l, m)
             + 0.125 * (group(a, b, d, e) + group(b, c, e, f) + group(d, e, g, h) + group(e, f, h, i));
    if (pc.mode == 1u) {
        sum = min(sum / max(1.0 - luma(sum), 1e-3), vec3(60000.0)); // undo the Karis weighting on average
        // Soft threshold: a quadratic knee below `threshold`, linear above.
        float threshold = pc.params.x;
        float knee = max(pc.params.y, 1e-4);
        float bright = max(sum.r, max(sum.g, sum.b));
        float soft = clamp(bright - threshold + knee, 0.0, 2.0 * knee);
        soft = soft * soft / (4.0 * knee);
        sum *= max(soft, bright - threshold) / max(bright, 1e-5);
    }
    out_color = vec4(max(sum, vec3(0.0)), 1.0);
}
