#version 460
// Specula ID buffer, counting stage (no colour output; needs fragmentStoresAndAtomics).
// MODE 0, blended draws: depth tested against the opaque depth (never written); each pixel counts
//   once per id, deduplicated through the per-pixel `seen` slots, however many layers of the id
//   cover it and in whatever order the fragments run. Glass behind glass is counted too.
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

// Whether the scene shows this fragment: mesh.frag's discards, mirrored (both modes).
bool shown() {
    if (pc.faces == FACES_LAVA) {
        // shade_lava_lamp: shaded on the proxy's near faces, or on its far faces when the camera
        // is inside the box.
        vec3 cam = pc.frame.camera_time.xyz;
        bool near_face = dot(v_normal, cam - v_world) > 0.0;
        mat4 m = pc.draws.items[v_ids.y].model;
        vec3 scale = vec3(length(m[0].xyz), length(m[1].xyz), length(m[2].xyz));
        mat3 rot = mat3(m[0].xyz / scale.x, m[1].xyz / scale.y, m[2].xyz / scale.z);
        vec3 ro = transpose(rot) * (cam - m[3].xyz);
        bool inside_box = all(lessThan(abs(ro), 0.5 * scale));
        return near_face != inside_box;
    }
    return true;
}

// Claims a slot of this pixel for `id`: true when the id wasn't counted here yet (or, past
// SEEN_SLOTS other ids at this pixel, always: an upper bound the CPU clamps). A slot holds
// (capture epoch << SEEN_ID_BITS) | id; one from another capture's epoch is free, so the slots
// never need clearing between captures.
bool first_here(uint id, uvec2 p) {
    if (id >= (1u << SEEN_ID_BITS)) {
        return true; // beyond the slots' id range: counted without dedupe
    }
    uint mine = (pc.epoch << SEEN_ID_BITS) | id;
    uint at = (p.y * pc.width + p.x) * SEEN_SLOTS;
    for (uint k = 0u; k < SEEN_SLOTS; k++) {
        uint cur = pc.seen.slot[at + k];
        for (uint tries = 0u; tries < 4u; tries++) {
            if ((cur >> SEEN_ID_BITS) == pc.epoch) {
                if (cur == mine) {
                    return false; // already counted here
                }
                break; // another id holds this slot this capture: try the next one
            }
            uint prev = atomicCompSwap(pc.seen.slot[at + k], cur, mine);
            if (prev == cur) {
                return true; // claimed a stale slot: first time here
            }
            cur = prev; // someone got there first: look again
        }
    }
    return true;
}

void main() {
    uint v_id = v_ids.x;
    if (v_id == 0u || !shown()) {
        return;
    }
    uint base = (v_id - 1u) * STATS_STRIDE;
    if (MODE == 1u) {
        pc.stats.v[base + 5u] = 1u; // idempotent: no atomic needed
        return;
    }
    uvec2 p = uvec2(gl_FragCoord.xy);
    if (!first_here(v_id, p)) {
        return;
    }
    atomicAdd(pc.stats.v[base], 1u);
    atomicMax(pc.stats.v[base + 1u], ~p.x);
    atomicMax(pc.stats.v[base + 2u], ~p.y);
    atomicMax(pc.stats.v[base + 3u], p.x);
    atomicMax(pc.stats.v[base + 4u], p.y);
}
