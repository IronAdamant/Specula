#version 460
// Bloom upsample: a 3x3 tent over the blur of the smaller level (src) plus this level's
// downsample (src2). params.x: tent radius in texels of the smaller level.
// Compile with shaders/build.sh.
#include "post.glsl"

vec3 tap(vec2 offset) {
    return texture(textures[pc.src], v_uv + offset * pc.texel * pc.params.x).rgb;
}

void main() {
    vec3 c = 4.0 * tap(vec2(0.0));
    c += 2.0 * (tap(vec2(-1.0, 0.0)) + tap(vec2(1.0, 0.0)) + tap(vec2(0.0, -1.0)) + tap(vec2(0.0, 1.0)));
    c += tap(vec2(-1.0, -1.0)) + tap(vec2(1.0, -1.0)) + tap(vec2(-1.0, 1.0)) + tap(vec2(1.0, 1.0));
    out_color = vec4(c / 16.0 + texture(textures[pc.src2], v_uv).rgb, 1.0);
}
