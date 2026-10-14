#version 460
// Screen-space ambient occlusion (src/ssao.rs): McGuire's Scalable Ambient Obscurance estimator
// on the SSAO depth image (src, same size as this target), with the normal reconstructed from
// depth and the sample spiral rotated per pixel by a 4x4 interleaved pattern (deterministic).
// mode: samples (bits 0..7), reversed Z (bit 8). params: radius (m), intensity, pixels per metre
// at a view depth of 1 m. Out: r = ambient visibility (1 = open), g = view depth (m; FAR = sky).
// Compile with shaders/build.sh.
#include "post.glsl"

const float FAR = 65000.0;
const float TAU = 6.2831853;
const float TURNS = 7.0; // turns of the sample spiral
// 4x4 Bayer order: neighbouring pixels get well-spread rotations, every 4x4 block all 16.
const uint BAYER[16] = uint[](0u, 8u, 2u, 10u, 12u, 4u, 14u, 6u, 3u, 11u, 1u, 9u, 15u, 7u, 13u, 5u);

ivec2 g_size;
bool g_reverse;

float depth_at(ivec2 p) {
    return texelFetch(textures[pc.src], clamp(p, ivec2(0), g_size - 1), 0).r;
}

bool is_sky(float d) {
    return g_reverse ? d <= 0.0 : d >= 1.0;
}

vec3 world_at(ivec2 p, float d) {
    vec2 ndc = (vec2(p) + 0.5) * pc.texel * 2.0 - 1.0;
    vec4 w = pc.frame.inv_view_proj * vec4(ndc, d, 1.0);
    return w.xyz / w.w;
}

void main() {
    g_size = textureSize(textures[pc.src], 0);
    g_reverse = (pc.mode & 256u) != 0u;
    ivec2 c = ivec2(gl_FragCoord.xy);
    float d = depth_at(c);
    if (is_sky(d)) {
        out_color = vec4(1.0, FAR, 0.0, 1.0);
        return;
    }
    vec3 p = world_at(c, d);
    float z = (pc.frame.view_proj * vec4(p, 1.0)).w;

    // Normal from the nearer neighbour on each axis (no smearing across silhouettes).
    float dl = depth_at(c - ivec2(1, 0)), dr = depth_at(c + ivec2(1, 0));
    float du = depth_at(c - ivec2(0, 1)), dd = depth_at(c + ivec2(0, 1));
    vec3 dx = abs(dr - d) < abs(d - dl) ? world_at(c + ivec2(1, 0), dr) - p : p - world_at(c - ivec2(1, 0), dl);
    vec3 dy = abs(dd - d) < abs(d - du) ? world_at(c + ivec2(0, 1), dd) - p : p - world_at(c - ivec2(0, 1), du);
    vec3 n = cross(dy, dx);
    float len = length(n);
    if (len < 1e-12) {
        out_color = vec4(1.0, z, 0.0, 1.0);
        return;
    }
    n /= len;
    if (dot(n, pc.frame.camera_time.xyz - p) < 0.0) {
        n = -n;
    }

    float radius = pc.params.x;
    float r2 = radius * radius;
    float screen = min(radius * pc.params.z / max(z, 1e-4), 96.0); // sample disc radius in pixels
    if (screen < 1.0) {
        out_color = vec4(1.0, z, 0.0, 1.0);
        return;
    }
    uint samples = pc.mode & 255u;
    float rot = (float(BAYER[(uint(c.y) & 3u) * 4u + (uint(c.x) & 3u)]) + 0.5) / 16.0 * TAU;
    float bias = 0.02 * radius;
    float eps = 0.01 * r2;
    float sum = 0.0;
    for (uint i = 0u; i < samples; i++) {
        float alpha = (float(i) + 0.5) / float(samples);
        float angle = alpha * TURNS * TAU + rot;
        ivec2 q = c + ivec2(round(vec2(cos(angle), sin(angle)) * (alpha * screen)));
        float dq = depth_at(q);
        if (is_sky(dq)) {
            continue;
        }
        vec3 v = world_at(clamp(q, ivec2(0), g_size - 1), dq) - p;
        float vv = dot(v, v);
        float f = max(r2 - vv, 0.0);
        sum += f * f * f * max((dot(v, n) - bias) / (eps + vv), 0.0);
    }
    float visibility = max(0.0, 1.0 - sum * pc.params.y * 5.0 / (r2 * r2 * r2 * float(samples)));
    out_color = vec4(visibility, z, 0.0, 1.0);
}
