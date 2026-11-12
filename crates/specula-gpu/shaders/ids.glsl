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
    uint seq_at;   // ids.items[seq_at + record]: the record's draw number; [2 seq_at + id - 1]: the
                   // id's last draw number (src/idbuffer.rs, "Covers")
    uint cover;    // 1: covers are written and honoured (ids_core.glsl)
} pc;

// Which fragments of a draw the scene shows, mirroring mesh.frag's discards (src/idbuffer.rs).
const uint FACES_ALL = 0u;   // every fragment: opaque draws, glass (both passes), blob shadows
const uint FACES_LAVA = 2u;  // the lava lamp's proxy: near faces, or far faces from inside it
// A flag beside them: the batch is `Lit` meshes, whose records may carry a wind term (sway and
// fade, frame.glsl); other materials use those record bits for themselves.
const uint FACES_FOLIAGE = 4u;
// A flag beside them: glass, whose fragments paint over what lies under them at COVER_ALPHA
// (ids_count.glsl mirrors shade_glass's alpha).
const uint FACES_COVER_GLASS = 8u;

// This capture's counts (see ids_core.glsl).
IdTarget id_target() {
    IdTarget t;
    t.stats = pc.stats;
    t.seen = pc.seen;
    t.width = pc.width;
    t.epoch = pc.epoch;
    t.height = pc.height;
    t.cover = pc.cover;
    return t;
}

// A mesh record's draw number in the colour pass's order.
uint record_seq(uint record) {
    return pc.ids.items[pc.seq_at + record];
}

// The last draw number of `id` (its draws' largest).
uint id_last_seq(uint id) {
    return pc.ids.items[2u * pc.seq_at + id - 1u];
}

// Whether pixel `p` was painted over after `id`'s last draw there (src/idbuffer.rs, "Covers").
bool id_hidden(uint id, uvec2 p) {
    return pc.cover != 0u && id_cover_seq(id_target(), p) > id_last_seq(id);
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

// Foliage cards (a FACES_FOLIAGE batch) also fade edge-on, as the scene cuts them (frame.glsl
// foliage_edge): `normal` and `world` are the fragment's.
bool cut_out(uint record, vec2 uv, vec3 normal, vec3 world) {
    uvec4 ext = pc.draws.items[record].ext;
    uint base_slot = ext.x & 0xffffu;
    if (base_slot == 0u || (ext.w & EXT_ALPHA_MASK) == 0u) {
        return false;
    }
    float edge = (pc.faces & FACES_FOLIAGE) != 0u ? foliage_edge(ext.w, normal, world, pc.frame.camera_time.xyz) : 1.0;
    return texture(textures[nonuniformEXT(base_slot - 1u)], uv).a * edge < float((ext.w >> 8) & 255u) / 255.0;
}
#endif
