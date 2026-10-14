#version 460
// Screen-space ambient occlusion (src/ssao.rs): McGuire's Scalable Ambient Obscurance estimator
// on the scene's depth (src, full resolution; this target is params.w times smaller), with the
// normal reconstructed from depth and the sample spiral rotated and scaled per pixel by
// interleaved gradient noise (deterministic). texel: 1 / the depth's size. mode: samples (bits
// 0..7), reversed Z (bit 8). params: radius (m), intensity, depth pixels per metre (at a view
// depth of 1 m for perspective; everywhere for orthographic), depth pixels per target pixel.
// pad: 0 for a perspective projection; for an orthographic one, the float bits of NDC depth per
// metre. Out: r = ambient visibility (1 = open), g = view depth (m from the camera, or from the
// near plane for orthographic; FAR = sky).
// Compile with shaders/build.sh.
#include "post.glsl"

const float FAR = 65000.0;
const float TAU = 6.2831853;
const float TURNS = 7.0; // turns of the sample spiral
// Interleaved gradient noise in [0, 1): neighbouring pixels get well-spread values, no tile.
float ign(vec2 p) {
    return fract(52.9829189 * fract(dot(p, vec2(0.06711056, 0.00583715))));
}

ivec2 g_size;
bool g_reverse;
float g_ortho; // NDC depth per metre of an orthographic projection, 0 for perspective

float depth_at(ivec2 p) {
    return texelFetch(textures[pc.src], clamp(p, ivec2(0), g_size - 1), 0).r;
}

// View depth in metres of the point at `ndc` with depth `d`. Perspective: clip w (the homogeneous
// point inv_view_proj * (ndc, d, 1) has w = 1 / clip w, so only its last row is needed).
// Orthographic: clip w is 1 everywhere; depth is linear, so the distance from the near plane.
float view_depth_at(Frame f, vec2 ndc, float d) {
    if (g_ortho > 0.0) {
        return abs(d - (g_reverse ? 1.0 : 0.0)) / g_ortho;
    }
    mat4 m = f.inv_view_proj;
    return 1.0 / (m[0][3] * ndc.x + m[1][3] * ndc.y + m[2][3] * d + m[3][3]);
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
    g_ortho = uintBitsToFloat(pc.pad);
    ivec2 own = ivec2(gl_FragCoord.xy); // this target's pixel (picks the pattern)
    ivec2 c = ivec2(gl_FragCoord.xy * pc.params.w); // the depth pixel under its centre
    float d = depth_at(c);
    if (is_sky(d)) {
        out_color = vec4(1.0, FAR, 0.0, 1.0);
        return;
    }
    vec3 p = world_at(c, d);
    float z = view_depth_at(pc.frame, (vec2(c) + 0.5) * pc.texel * 2.0 - 1.0, d);

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
    // Towards the camera: the point of this pixel's ray on the near plane (any projection).
    if (dot(n, world_at(c, g_reverse ? 1.0 : 0.0) - p) < 0.0) {
        n = -n;
    }

    float radius = pc.params.x;
    float r2 = radius * radius;
    // Sample disc radius in depth pixels (orthographic: the same at every depth).
    float screen = min(radius * pc.params.z / (g_ortho > 0.0 ? 1.0 : max(z, 1e-4)), 128.0);
    if (screen < 1.0) {
        out_color = vec4(1.0, z, 0.0, 1.0);
        return;
    }
    uint samples = pc.mode & 255u;
    // Interleaved gradient noise (Jimenez 2014) rotates the spiral and shifts its radii per pixel.
    vec2 o = vec2(own);
    float rot = ign(o) * TAU;
    float jitter = ign(o + vec2(5.588238, 5.588238));
    float bias = 0.02 * radius;
    float eps = 0.01 * r2;
    float sum = 0.0;
    for (uint i = 0u; i < samples; i++) {
        float alpha = (float(i) + jitter) / float(samples);
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
