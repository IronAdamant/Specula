#version 460
// Specula ID buffer, counting stage (no colour output; needs fragmentStoresAndAtomics).
// MODE 0, blended draws: depth tested against the opaque depth (never written), each id counts
//   its own front-facing fragments into its stats, so glass behind glass is counted too.
// MODE 1, every draw, no depth test: marks each id rasterized at all (an id with no visible pixel
//   but rasterized fragments is occluded; one never rasterized is sub-pixel or culled).
// See src/idbuffer.rs. Compile with shaders/build.sh.
#include "ids.glsl"

// Side effects only for fragments that pass the depth test.
layout(early_fragment_tests) in;

layout(constant_id = 0) const uint MODE = 0u;

layout(location = 0) flat in uint v_id;
layout(location = 1) in vec3 v_world;
layout(location = 2) in vec3 v_normal;

void main() {
    if (v_id == 0u) {
        return;
    }
    uint base = (v_id - 1u) * STATS_STRIDE;
    if (MODE == 1u) {
        pc.stats.v[base + 5u] = 1u; // idempotent: no atomic needed
        return;
    }
    // Front faces only, chosen by normal like glass in mesh.frag (winding varies by mesh): a
    // closed shell covers each pixel with exactly one of them.
    if (dot(v_normal, pc.frame.camera_time.xyz - v_world) <= 0.0) {
        return;
    }
    uvec2 p = uvec2(gl_FragCoord.xy);
    atomicAdd(pc.stats.v[base], 1u);
    atomicMax(pc.stats.v[base + 1u], ~p.x);
    atomicMax(pc.stats.v[base + 2u], ~p.y);
    atomicMax(pc.stats.v[base + 3u], p.x);
    atomicMax(pc.stats.v[base + 4u], p.y);
}
