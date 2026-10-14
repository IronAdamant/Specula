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

// Must match `IdPush` in src/idbuffer.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;   // instance i reads items[gl_InstanceIndex]
    Ids ids;       // and ids.items[gl_InstanceIndex]
    Stats stats;
    uint faces;    // counting passes: which faces count (FACES_*), set per batch
    uint pad;
} pc;

// Which fragments of a draw count, as mesh.frag shades them (see src/idbuffer.rs `FACES_*`).
const uint FACES_ALL = 0u;   // every fragment: blob shadows, open glass, opaque draws
const uint FACES_FRONT = 1u; // front faces by normal: closed glass shells (one per pixel)
const uint FACES_LAVA = 2u;  // the lava lamp's proxy: near faces, or far faces from inside it
