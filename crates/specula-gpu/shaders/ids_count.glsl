// Specula ID buffer, blended draws (no colour output; needs fragmentStoresAndAtomics).
// MODE 0, insert: depth tested against the opaque depth (never written); the fragment's id joins
//   its pixel's set of the SEEN_SLOTS smallest ids. The final sets don't depend on the
//   order fragments run in; ids_resolve.frag counts them afterwards (one count per id per pixel).
//   An id pushed out of a set (more than SEEN_SLOTS ids at the pixel) is marked incomplete: its
//   resolved count and box are then lower bounds. Equally reproducible, and nothing is counted
//   here (no contended atomics, whatever the number of layers).
// MODE 1, raster: every draw, no depth test: marks each id rasterized at all.
// Included by ids_count.frag and ids_count_tex.frag (TEXTURED: textured draws, whose alpha-masked
// texels the scene cuts). See src/idbuffer.rs. Compile with shaders/build.sh.
#include "ids.glsl"

// Side effects only for fragments that pass the depth test.
layout(early_fragment_tests) in;

layout(constant_id = 0) const uint MODE = 0u;

layout(location = 0) flat in uvec2 v_ids; // x: the id, y: the record index
layout(location = 1) in vec3 v_world;
layout(location = 2) in vec3 v_normal;
#ifdef TEXTURED
layout(location = 3) in vec2 v_uv;
#endif

// Whether the scene shows this fragment: mesh.frag's discards, mirrored (every mode).
bool shown() {
#ifdef TEXTURED
    if (cut_out(v_ids.y, v_uv)) {
        return false; // an alpha-masked texel (mesh_frag.glsl discards it)
    }
#endif
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

// Sets flag word `i` (idempotent, so no atomic). Every fragment of an id targets the same word;
// iGPUs serialise stores to one address, so it is read first and written only while still 0
// (once set, the readers share the cached line).
void set_flag(uint i) {
    if (pc.stats.v[i] == 0u) {
        pc.stats.v[i] = 1u;
    }
}

// `id` is missing from some pixel's final set although drawn there.
void incomplete(uint id) {
    set_flag(stats_base(id) + 6u);
}

// Inserts `id` into the pixel's set of the SEEN_SLOTS smallest ids, with at most one successful
// atomic whatever order the ids arrive in (blended draws come back to front, so often largest
// first). The set is unordered; claimed slots always form a prefix (a fragment claims the first
// free slot only). Per attempt, from one read of the slots:
// - the id is there: done;
// - a slot is free: claim the first free one (atomicCompSwap);
// - the set is full and the id is smaller than its largest: replace the largest
//   (atomicCompSwap), which is out for good: SEEN_SLOTS smaller ids hold the slots, and a slot
//   only ever decreases within a capture (a stale read only ever shows larger values, so a
//   successful swap really did replace the largest);
// - otherwise the id is out for good (SEEN_SLOTS smaller ids are there).
// A failed swap means a slot was claimed or decreased meanwhile: decide again with the value it
// returned (this ends: each slot changes at most once per id at the pixel). So the final set is
// the SEEN_SLOTS smallest ids drawn at the pixel, whatever the order; an id out of it is incomplete.
void insert(uint id, uvec2 p) {
    if (id >= (1u << SEEN_ID_BITS)) {
        incomplete(id); // beyond the slots' id range: never in a set (0 pixels, at least)
        return;
    }
    const uint mask = (1u << SEEN_ID_BITS) - 1u;
    uint v = (pc.epoch << SEEN_ID_BITS) | id;
    uint at = seen_at(p);
    // One plain read; afterwards a failed swap's returned value refreshes that slot (a plain
    // re-read could keep returning the same stale cached value, and spin forever).
    uint s[SEEN_SLOTS];
    for (uint k = 0u; k < SEEN_SLOTS; k++) {
        s[k] = pc.seen.slot[at + k];
    }
    // Ends within (ids at the pixel) x SEEN_SLOTS failures; the cap only guards the GPU against
    // a hang (a hung shader loses the device) and is never reached.
    for (uint tries = 0u; tries < 1024u; tries++) {
        uint free_at = SEEN_SLOTS;
        uint max_at = 0u;
        for (uint k = 0u; k < SEEN_SLOTS; k++) {
            if (s[k] == v) {
                return; // already in the set
            }
            if (!seen_current(s[k])) {
                free_at = min(free_at, k);
            } else if (s[k] > s[max_at]) { // (only used when every slot is claimed)
                max_at = k;
            }
        }
        uint j = free_at < SEEN_SLOTS ? free_at : max_at;
        if (free_at == SEEN_SLOTS && v > s[max_at]) {
            incomplete(id); // SEEN_SLOTS smaller ids hold the slots
            return;
        }
        uint prev = atomicCompSwap(pc.seen.slot[at + j], s[j], v);
        if (prev == s[j]) {
            if (free_at == SEEN_SLOTS) {
                incomplete(prev & mask); // replaced the largest: out for good
            }
            return;
        }
        s[j] = prev;
    }
    incomplete(id);
}

void main() {
    uint id = v_ids.x;
    if (id == 0u || !shown()) {
        return;
    }
    if (MODE == 1u) {
        set_flag(stats_base(id) + 5u);
    } else {
        insert(id, uvec2(gl_FragCoord.xy));
    }
}
