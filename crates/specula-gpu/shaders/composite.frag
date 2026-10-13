#version 460
// Tone mapping: the exposed HDR scene plus bloom (src2, scaled by params.x) -> display values,
// with the same ACES curve and dither as the forward path (shaders/mesh.frag `display`).
// Compile with shaders/build.sh.
#include "post.glsl"

uint pcg(uint v) {
    uint state = v * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

float hash_u(uint a, uint b) { return float(pcg(a + pcg(b)) >> 8) / 16777216.0; }

vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

void main() {
    vec3 c = texture(textures[pc.src], v_uv).rgb;
    if (pc.src2 != 0xffffffffu) {
        c += texture(textures[pc.src2], v_uv).rgb * pc.params.x;
    }
    if (pc.frame.sky.w > 0.5) {
        c = aces(c);
        float n = hash_u(uint(gl_FragCoord.x), uint(gl_FragCoord.y)) - 0.5;
        c += n * 2.2 * pow(max(c, vec3(1e-4)), vec3(0.545)) / 255.0;
    }
    out_color = vec4(max(c, vec3(0.0)), 1.0);
}
