#version 460
// SSAO blur (src/ssao.rs), one direction of a separable 9-tap Gaussian (mode 0: across, 1: down)
// over src (r = visibility, g = view depth). Each tap is weighted by how far its depth lies from
// the plane through this pixel (the depth slope along the blur, from the two neighbours), so
// occlusion doesn't bleed across depth edges while slanted surfaces still blur fully. The
// tolerance is 2% of the depth, or params.x metres when set (orthographic: depth is measured
// from the near plane, so a relative tolerance means nothing).
// Compile with shaders/build.sh.
#include "post.glsl"

const float FAR = 65000.0;
// Gaussian, sigma 2.5 texels, taps 0..4.
const float WEIGHTS[5] = float[](0.1592, 0.1468, 0.1153, 0.0771, 0.0439);

void main() {
    ivec2 size = textureSize(textures[pc.src], 0);
    ivec2 c = ivec2(gl_FragCoord.xy);
    ivec2 axis = pc.mode == 0u ? ivec2(1, 0) : ivec2(0, 1);
    vec2 center = texelFetch(textures[pc.src], c, 0).rg;
    if (center.g >= 0.5 * FAR) {
        out_color = vec4(1.0, center.g, 0.0, 1.0);
        return;
    }
    float z = center.g;
    float zl = texelFetch(textures[pc.src], clamp(c - axis, ivec2(0), size - 1), 0).g;
    float zr = texelFetch(textures[pc.src], clamp(c + axis, ivec2(0), size - 1), 0).g;
    // The slope of the nearer side (a silhouette's far side would tilt the plane).
    float slope = abs(zr - z) < abs(z - zl) ? zr - z : z - zl;
    float tolerance = pc.params.x > 0.0 ? pc.params.x : 0.02 * z;
    float sum = center.r * WEIGHTS[0];
    float weight = WEIGHTS[0];
    for (int i = 1; i <= 4; i++) {
        for (int side = -1; side <= 1; side += 2) {
            int k = i * side;
            vec2 s = texelFetch(textures[pc.src], clamp(c + axis * k, ivec2(0), size - 1), 0).rg;
            float off_plane = abs(s.g - (z + slope * float(k)));
            float w = WEIGHTS[i] * max(0.0, 1.0 - off_plane / tolerance);
            sum += s.r * w;
            weight += w;
        }
    }
    out_color = vec4(sum / weight, z, 0.0, 1.0); // weight >= the centre's
}
