#version 460
// Specula ID buffer, resolve: one fragment per pixel (a full-screen triangle) counts each id in
// the pixel's final set once: its pixel count and its box, with atomics only where the pixel is
// on the edge of the id's region (a neighbour without the id, or the capture's border): the
// extremes lie there, every horizontal run has one left and one right edge, and edges are far
// fewer than pixels (contended atomics on one address are what iGPUs are slowest at).
// Sums, minima and maxima don't depend on the order fragments run in, so the counts are
// reproducible. See src/idbuffer.rs. Compile with shaders/build.sh.
#include "ids.glsl"

layout(location = 0) in vec2 v_uv; // from fullscreen.vert (the pixel comes from gl_FragCoord)

// Whether slot value `v` (current epoch) is in pixel `q`'s set.
bool has(uint v, uvec2 q) {
    uint at = seen_at(q);
    for (uint k = 0u; k < SEEN_SLOTS; k++) {
        if (pc.seen.slot[at + k] == v) {
            return true;
        }
    }
    return false;
}

void main() {
    if (v_uv.x < -1.0) {
        return; // never: keeps the interface matching fullscreen.vert's output
    }
    uvec2 p = uvec2(gl_FragCoord.xy);
    uint at = seen_at(p);
    for (uint k = 0u; k < SEEN_SLOTS; k++) {
        uint v = pc.seen.slot[at + k];
        if (!seen_current(v)) {
            return; // free slots sort last
        }
        uint id = v & ((1u << SEEN_ID_BITS) - 1u);
        bvec4 edges = bvec4(
            p.x == 0u || !has(v, p - uvec2(1u, 0u)),
            p.y == 0u || !has(v, p - uvec2(0u, 1u)),
            p.x + 1u >= pc.width || !has(v, p + uvec2(1u, 0u)),
            p.y + 1u >= pc.height || !has(v, p + uvec2(0u, 1u)));
        // Pixels by runs: each horizontal run of the id adds (its end + 1) at its right edge and
        // subtracts its start at its left edge (mod 2^32: the sum is exact).
        uint run = (edges.z ? p.x + 1u : 0u) - (edges.x ? p.x : 0u);
        if (run != 0u) {
            atomicAdd(pc.stats.v[stats_base(id)], run);
        }
        count_box(id, p, edges);
    }
}
