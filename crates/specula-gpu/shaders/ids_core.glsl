// Sight's ID buffer: the buffers and the per-pixel set insert, shared by every ID stage (mesh and
// 2D). Included after frame.glsl (buffer references); not compiled on its own. See
// src/idbuffer.rs.
#ifndef IDS_CORE_GLSL
#define IDS_CORE_GLSL

// One id per draw record, in record order (0 = not counted).
layout(buffer_reference, std430, buffer_reference_align = 4) readonly buffer Ids {
    uint items[];
};

// Cleared to 0 each capture. Per id, STATS_STRIDE uints from stats_base(id):
//   [0] blended pixels (resolve)
//   [1] ~min x, [2] ~min y (stored inverted, so atomicMax finds the minimum), [3] max x, [4] max y
//   [5] 1 when any fragment of the id was rasterized (no depth test)
//   [6] 1 when the id was pushed out of some pixel's set it was drawn in (or can't enter one):
//       its count and box are then lower bounds (plain stores of 1: same-value races are benign)
layout(buffer_reference, std430, buffer_reference_align = 4) buffer Stats {
    uint v[];
};
const uint STATS_STRIDE = 7u;
uint stats_base(uint id) {
    return (id - 1u) * STATS_STRIDE;
}

// Per pixel, the SEEN_SLOTS smallest blended ids drawn there this capture (unordered; claimed
// slots form a prefix), each (epoch << SEEN_ID_BITS) | id; a slot of another capture's epoch is
// free. Must match `SEEN_SLOTS` / `SEEN_ID_BITS`.
// Not `coherent`: within a pass every write is an atomic and a plain read may be stale (the
// insert swaps with atomicCompSwap against what it read; claims are permanent and a slot only
// decreases, so a stale read only ever shows a slot free or larger than it is); between passes
// the barriers make writes visible. Coherent loads cost an iGPU dearly.
layout(buffer_reference, std430, buffer_reference_align = 4) buffer Seen {
    uint slot[];
};
const uint SEEN_SLOTS = 4u;
const uint SEEN_ID_BITS = 20u;

// Where a capture's counts go.
struct IdTarget {
    Stats stats;
    Seen seen;
    uint width; // pixels per row (indexes `seen`)
    uint epoch; // this capture's tag in `seen` (1..4095)
};

uint id_seen_at(IdTarget t, uvec2 p) {
    return (p.y * t.width + p.x) * SEEN_SLOTS;
}

bool id_seen_current(IdTarget t, uint v) {
    return (v >> SEEN_ID_BITS) == t.epoch;
}

// Sets flag word `i` (idempotent, so no atomic). Every fragment of an id targets the same word;
// iGPUs serialise stores to one address, so it is read first and written only while still 0
// (once set, the readers share the cached line).
void id_set_flag(IdTarget t, uint i) {
    if (t.stats.v[i] == 0u) {
        t.stats.v[i] = 1u;
    }
}

// `id` is missing from some pixel's final set although drawn there.
void id_incomplete(IdTarget t, uint id) {
    id_set_flag(t, stats_base(id) + 6u);
}

// `id` was rasterized at all (no depth test).
void id_rasterized(IdTarget t, uint id) {
    id_set_flag(t, stats_base(id) + 5u);
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
void id_insert(IdTarget t, uint id, uvec2 p) {
    if (id >= (1u << SEEN_ID_BITS)) {
        id_incomplete(t, id); // beyond the slots' id range: never in a set (0 pixels, at least)
        return;
    }
    const uint mask = (1u << SEEN_ID_BITS) - 1u;
    uint v = (t.epoch << SEEN_ID_BITS) | id;
    uint at = id_seen_at(t, p);
    // One plain read; afterwards a failed swap's returned value refreshes that slot (a plain
    // re-read could keep returning the same stale cached value, and spin forever).
    uint s[SEEN_SLOTS];
    for (uint k = 0u; k < SEEN_SLOTS; k++) {
        s[k] = t.seen.slot[at + k];
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
            if (!id_seen_current(t, s[k])) {
                free_at = min(free_at, k);
            } else if (s[k] > s[max_at]) { // (only used when every slot is claimed)
                max_at = k;
            }
        }
        uint j = free_at < SEEN_SLOTS ? free_at : max_at;
        if (free_at == SEEN_SLOTS && v > s[max_at]) {
            id_incomplete(t, id); // SEEN_SLOTS smaller ids hold the slots
            return;
        }
        uint prev = atomicCompSwap(t.seen.slot[at + j], s[j], v);
        if (prev == s[j]) {
            if (free_at == SEEN_SLOTS) {
                id_incomplete(t, prev & mask); // replaced the largest: out for good
            }
            return;
        }
        s[j] = prev;
    }
    id_incomplete(t, id);
}

#endif
