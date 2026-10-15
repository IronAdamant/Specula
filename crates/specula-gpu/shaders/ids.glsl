// Sight's ID buffer: what every ID stage shares (see src/idbuffer.rs). Included by the ids*
// entry files (ids_vert.glsl's variants, ids*.frag); not compiled on its own. Defines: SKINNED
// (joints and weights, posed like mesh.vert), TEXTURED (UVs, for the alpha-mask cut).
#ifdef TEXTURED
#extension GL_EXT_nonuniform_qualifier : require
#endif
#include "frame.glsl"

#include "ids_core.glsl"

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
    Bones bones;   // skinned draws: the palette starts at draws.items[i].ext.z (as mesh.vert)
} pc;

// Which fragments of a draw the scene shows, mirroring mesh.frag's discards (src/idbuffer.rs).
const uint FACES_ALL = 0u;   // every fragment: opaque draws, glass (both passes), blob shadows
const uint FACES_LAVA = 2u;  // the lava lamp's proxy: near faces, or far faces from inside it

// This capture's counts (see ids_core.glsl).
IdTarget id_target() {
    IdTarget t;
    t.stats = pc.stats;
    t.seen = pc.seen;
    t.width = pc.width;
    t.epoch = pc.epoch;
    return t;
}

uint seen_at(uvec2 p) {
    return id_seen_at(id_target(), p);
}

bool seen_current(uint v) {
    return id_seen_current(id_target(), v);
}

// Raises `id`'s box to pixel `p` on the sides `edges` marks (the resolve calls it only for
// pixels on the edge of the id's region: contended atomics on one address are what iGPUs are
// slowest at, and a region's edges are far fewer than its pixels).
void count_box(uint id, uvec2 p, bvec4 edges) {
    uint base = stats_base(id);
    if (edges.x) atomicMax(pc.stats.v[base + 1u], ~p.x); // left edge
    if (edges.y) atomicMax(pc.stats.v[base + 2u], ~p.y); // top edge
    if (edges.z) atomicMax(pc.stats.v[base + 3u], p.x);  // right edge
    if (edges.w) atomicMax(pc.stats.v[base + 4u], p.y);  // bottom edge
}

#ifdef TEXTURED
// Alpha-masked textured materials (glTF MASK): the texels the scene cuts (mesh_frag.glsl,
// shadow_mask.frag): base colour alpha below the draw's cutoff. Same texture, UVs and LOD (the
// same rasterization, so the same derivatives).
layout(set = 0, binding = 1) uniform sampler2D textures[];

bool cut_out(uint record, vec2 uv) {
    uvec4 ext = pc.draws.items[record].ext;
    uint base_slot = ext.x & 0xffffu;
    if (base_slot == 0u || (ext.w & EXT_ALPHA_MASK) == 0u) {
        return false;
    }
    return texture(textures[nonuniformEXT(base_slot - 1u)], uv).a < float((ext.w >> 8) & 255u) / 255.0;
}
#endif
