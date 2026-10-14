// Sight's ID buffer: what every ID stage shares (see src/idbuffer.rs). Included by ids.vert,
// ids.frag and ids_count.frag; not compiled on its own.
#include "frame.glsl"

// One id per draw record, in record order (0 = not counted).
layout(buffer_reference, std430, buffer_reference_align = 4) readonly buffer Ids {
    uint items[];
};

// Per id (index id - 1), STATS_STRIDE uints, cleared to 0 each capture:
//   [0] blended pixels (front-facing fragments that pass the opaque depth test)
//   [1] ~min x, [2] ~min y (stored inverted, so atomicMax finds the minimum), [3] max x, [4] max y
//   [5] 1 when any fragment of the id was rasterized (no depth test)
layout(buffer_reference, std430, buffer_reference_align = 4) buffer Stats {
    uint v[];
};
const uint STATS_STRIDE = 6u;

// Per pixel, SEEN_SLOTS blended ids already counted there, each (epoch << SEEN_ID_BITS) | id;
// a slot of another capture's epoch is free. Must match `SEEN_SLOTS` / `SEEN_ID_BITS`.
layout(buffer_reference, std430, buffer_reference_align = 4) coherent buffer Seen {
    uint slot[];
};
const uint SEEN_SLOTS = 2u;
const uint SEEN_ID_BITS = 20u;

// Must match `IdPush` in src/idbuffer.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;   // instance i reads items[gl_InstanceIndex]
    Ids ids;       // and ids.items[gl_InstanceIndex]
    Stats stats;
    Seen seen;     // SEEN_SLOTS ids per pixel already counted there (cleared each capture)
    uint faces;    // counting passes: which fragments the scene shows (FACES_*), set per batch
    uint width;    // pixels per row (indexes `seen`)
    uint epoch;    // this capture's tag in `seen` (1..4095)
    uint pad;
} pc;

// Which fragments of a draw the scene shows, mirroring mesh.frag's discards (src/idbuffer.rs).
const uint FACES_ALL = 0u;   // every fragment: opaque draws, glass (both passes), blob shadows
const uint FACES_LAVA = 2u;  // the lava lamp's proxy: near faces, or far faces from inside it
