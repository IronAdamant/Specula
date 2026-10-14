#version 460
// SSAO blur (src/ssao.rs): a 4x4 box (one period of ssao.frag's interleaved pattern, so the
// pattern averages out) over src (r = visibility, g = view depth), each tap weighted by how close
// its depth is to this pixel's, so occlusion doesn't bleed across depth edges.
// Compile with shaders/build.sh.
#include "post.glsl"

const float FAR = 65000.0;

void main() {
    ivec2 size = textureSize(textures[pc.src], 0);
    ivec2 c = ivec2(gl_FragCoord.xy);
    vec2 center = texelFetch(textures[pc.src], c, 0).rg;
    if (center.g >= 0.5 * FAR) {
        out_color = vec4(1.0, center.g, 0.0, 1.0);
        return;
    }
    float tolerance = 0.04 * center.g;
    float sum = 0.0;
    float weight = 0.0;
    for (int y = -2; y < 2; y++) {
        for (int x = -2; x < 2; x++) {
            vec2 s = texelFetch(textures[pc.src], clamp(c + ivec2(x, y), ivec2(0), size - 1), 0).rg;
            float w = max(0.0, 1.0 - abs(s.g - center.g) / tolerance);
            sum += s.r * w;
            weight += w;
        }
    }
    out_color = vec4(sum / weight, center.g, 0.0, 1.0); // weight >= 1: the centre is a tap
}
