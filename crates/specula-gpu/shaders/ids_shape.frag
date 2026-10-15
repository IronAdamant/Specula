#version 460
// Sight's ID buffer, vector shapes (see src/idbuffer.rs): the fragments the scene shows
// (shape.frag's cut: alpha 0, the outer edge of the anti-aliasing fringe), by MODE:
// 1 insert: they join their pixel's id set (shapes blend: depth tested, not written);
// 2 raster: they mark their id rasterized at all (no depth test).
// Compile with shaders/build.sh.
#define IDS
#include "sprite.glsl"

// Side effects only for fragments that pass the depth test.
layout(early_fragment_tests) in;

layout(constant_id = 0) const uint MODE = 1u;

layout(location = 0) in vec4 v_color;
layout(location = 1) in vec3 v_world;
layout(location = 2) flat in uint v_id;

layout(location = 0) out uint out_id;

void main() {
    if (v_world.x != v_world.x) {
        return; // never (NaN): keeps the interface matching the vertex stage's outputs
    }
    if (clamp(v_color.a, 0.0, 1.0) <= 0.0 || v_id == 0u) {
        discard; // the scene shows nothing here
    }
    IdTarget t;
    t.stats = pc.idp.stats;
    t.seen = pc.idp.seen;
    t.width = pc.idp.width;
    t.epoch = pc.idp.epoch;
    if (MODE == 1u) {
        id_insert(t, v_id, uvec2(gl_FragCoord.xy));
    } else if (MODE == 2u) {
        id_rasterized(t, v_id);
    } else {
        out_id = v_id;
    }
}
