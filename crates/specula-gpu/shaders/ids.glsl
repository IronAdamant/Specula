// Sight's ID buffer: what every ID stage shares (see src/idbuffer.rs). Included by ids.vert,
// ids_count.frag and ids_resolve.frag; not compiled on its own.
#include "frame.glsl"

// One id per draw record, in record order (0 = not counted).
layout(buffer_reference, std430, buffer_reference_align = 4) readonly buffer Ids {
    uint items[];
};

// Cleared to 0 each capture. v[0]: 1 when some pixel had more blended ids than SEEN_SLOTS (the
// overflow pass then runs). Then per id, STATS_STRIDE uints from stats_base(id):
//   [0] blended pixels
//   [1] ~min x, [2] ~min y (stored inverted, so atomicMax finds the minimum), [3] max x, [4] max y
//   [5] 1 when any fragment of the id was rasterized (no depth test)
//   [6] 1 when its count is approximate (counted by the overflow pass)
layout(buffer_reference, std430, buffer_reference_align = 4) buffer Stats {
    uint v[];
};
const uint STATS_STRIDE = 7u;
uint stats_base(uint id) {
    return 1u + (id - 1u) * STATS_STRIDE;
}

// Per pixel, the SEEN_SLOTS smallest blended ids drawn there this capture, ascending, each
// (epoch << SEEN_ID_BITS) | id; a slot of another capture's epoch is free (sorts last). Must
// match `SEEN_SLOTS` / `SEEN_ID_BITS`.
// Not `coherent`: within a pass every write is an atomic and a plain read may be stale (the
// insert re-checks with atomicCompSwap; a slot only decreases, so a stale "smaller id here" or
// "already here" stays true); between passes the barriers make writes visible. Coherent loads
// cost an iGPU dearly.
layout(buffer_reference, std430, buffer_reference_align = 4) buffer Seen {
    uint slot[];
};
const uint SEEN_SLOTS = 4u;
const uint SEEN_ID_BITS = 20u;

// Must match `IdPush` in src/idbuffer.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;   // instance i reads items[gl_InstanceIndex]
    Ids ids;       // and ids.items[gl_InstanceIndex]
    Stats stats;
    Seen seen;
    uint faces;    // counting passes: which fragments the scene shows (FACES_*), set per batch
    uint width;    // pixels per row (indexes `seen`)
    uint epoch;    // this capture's tag in `seen` (1..4095)
    uint height;   // rows of the capture
} pc;

// Which fragments of a draw the scene shows, mirroring mesh.frag's discards (src/idbuffer.rs).
const uint FACES_ALL = 0u;   // every fragment: opaque draws, glass (both passes), blob shadows
const uint FACES_LAVA = 2u;  // the lava lamp's proxy: near faces, or far faces from inside it

uint seen_at(uvec2 p) {
    return (p.y * pc.width + p.x) * SEEN_SLOTS;
}

bool seen_current(uint v) {
    return (v >> SEEN_ID_BITS) == pc.epoch;
}

// Counts pixel `p` for `id`: its pixel count and box, all with atomics. Only the overflow pass
// uses the box part per fragment (`count_pixel`); the resolve raises the box only from pixels on
// the edge of an id's region (`ids_resolve.frag`): contended atomics on one address are what
// iGPUs are slowest at, and a region's edges are far fewer than its pixels.
void count_box(uint id, uvec2 p, bvec4 edges) {
    uint base = stats_base(id);
    if (edges.x) atomicMax(pc.stats.v[base + 1u], ~p.x); // left edge
    if (edges.y) atomicMax(pc.stats.v[base + 2u], ~p.y); // top edge
    if (edges.z) atomicMax(pc.stats.v[base + 3u], p.x);  // right edge
    if (edges.w) atomicMax(pc.stats.v[base + 4u], p.y);  // bottom edge
}

void count_pixel(uint id, uvec2 p) {
    atomicAdd(pc.stats.v[stats_base(id)], 1u);
    count_box(id, p, bvec4(true));
}
