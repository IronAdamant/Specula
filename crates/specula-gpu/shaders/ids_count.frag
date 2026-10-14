#version 460
// Specula ID buffer, blended draws (no colour output; needs fragmentStoresAndAtomics).
// MODE 0, insert: depth tested against the opaque depth (never written); the fragment's id joins
//   its pixel's sorted set of the SEEN_SLOTS smallest ids. The final sets don't depend on the
//   order fragments run in; ids_resolve.frag counts them afterwards (one count per id per pixel).
// MODE 1, raster: every draw, no depth test: marks each id rasterized at all.
// MODE 2, overflow (only when some pixel had more ids than slots): depth tested again; each
//   fragment of an id missing from its pixel's final set counts that pixel (every layer: an upper
//   bound, marked approximate). The final sets are fixed by then, so this is reproducible too.
// See src/idbuffer.rs. Compile with shaders/build.sh.
#include "ids.glsl"

// Side effects only for fragments that pass the depth test.
layout(early_fragment_tests) in;

layout(constant_id = 0) const uint MODE = 0u;

layout(location = 0) flat in uvec2 v_ids; // x: the id, y: the record index
layout(location = 1) in vec3 v_world;
layout(location = 2) in vec3 v_normal;

// Whether the scene shows this fragment: mesh.frag's discards, mirrored (every mode).
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

void overflowed() {
    pc.stats.v[0] = 1u; // idempotent
}

// Inserts `id` into the pixel's ascending set of the SEEN_SLOTS smallest ids (free slots sort
// last). A slot's value only ever decreases within a capture; a value pushed out of a slot by a
// smaller one is carried on to the next slot, and one pushed past the last slot overflows. So
// the final set is the SEEN_SLOTS smallest ids, whatever the order.
void insert(uint id, uvec2 p) {
    if (id >= (1u << SEEN_ID_BITS)) {
        overflowed(); // beyond the slots' id range: the overflow pass counts it
        return;
    }
    uint v = (pc.epoch << SEEN_ID_BITS) | id;
    uint at = seen_at(p);
    for (uint k = 0u; k < SEEN_SLOTS; k++) {
        uint cur = pc.seen.slot[at + k];
        for (uint tries = 0u; tries < 64u; tries++) {
            bool free = !seen_current(cur);
            if (!free && cur == v) {
                return; // already in the set
            }
            if (!free && cur < v) {
                break; // a smaller id holds this slot: try the next one
            }
            uint prev = atomicCompSwap(pc.seen.slot[at + k], cur, v);
            if (prev == cur) {
                if (free) {
                    return;
                }
                v = cur; // pushed a larger id out: carry it on
                break;
            }
            cur = prev; // the slot changed under us: look again
        }
    }
    overflowed();
}

bool in_set(uint id, uvec2 p) {
    uint v = (pc.epoch << SEEN_ID_BITS) | id;
    uint at = seen_at(p);
    for (uint k = 0u; k < SEEN_SLOTS; k++) {
        if (pc.seen.slot[at + k] == v) {
            return true;
        }
    }
    return false;
}

void main() {
    uint id = v_ids.x;
    if (id == 0u || !shown()) {
        return;
    }
    uvec2 p = uvec2(gl_FragCoord.xy);
    if (MODE == 1u) {
        pc.stats.v[stats_base(id) + 5u] = 1u; // idempotent: no atomic needed
    } else if (MODE == 0u) {
        insert(id, p);
    } else if (id >= (1u << SEEN_ID_BITS) || !in_set(id, p)) {
        count_pixel(id, p);
        pc.stats.v[stats_base(id) + 6u] = 1u;
    }
}
