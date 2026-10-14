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

layout(location = 0) flat in uvec2 v_ids; // x: the id, y: the record index
layout(location = 1) in vec3 v_world;
layout(location = 2) in vec3 v_normal;

// Whether this fragment is one the scene shows, by the batch's face rule (both modes).
bool counted_face() {
    vec3 cam = pc.frame.camera_time.xyz;
    // Faces chosen by normal, as mesh.frag does (winding varies by mesh).
    bool front = dot(v_normal, cam - v_world) > 0.0;
    if (pc.faces == FACES_FRONT) {
        return front;
    }
    if (pc.faces == FACES_LAVA) {
        // mesh.frag shade_lava_lamp: shaded on the near faces, or on the far faces when the
        // camera is inside the proxy box.
        mat4 m = pc.draws.items[v_ids.y].model;
        vec3 scale = vec3(length(m[0].xyz), length(m[1].xyz), length(m[2].xyz));
        mat3 rot = mat3(m[0].xyz / scale.x, m[1].xyz / scale.y, m[2].xyz / scale.z);
        vec3 ro = transpose(rot) * (cam - m[3].xyz);
        bool inside_box = all(lessThan(abs(ro), 0.5 * scale));
        return front != inside_box;
    }
    return true;
}

void main() {
    uint v_id = v_ids.x;
    if (v_id == 0u || !counted_face()) {
        return;
    }
    uint base = (v_id - 1u) * STATS_STRIDE;
    if (MODE == 1u) {
        pc.stats.v[base + 5u] = 1u; // idempotent: no atomic needed
        return;
    }
    uvec2 p = uvec2(gl_FragCoord.xy);
    atomicAdd(pc.stats.v[base], 1u);
    atomicMax(pc.stats.v[base + 1u], ~p.x);
    atomicMax(pc.stats.v[base + 2u], ~p.y);
    atomicMax(pc.stats.v[base + 3u], p.x);
    atomicMax(pc.stats.v[base + 4u], p.y);
}
