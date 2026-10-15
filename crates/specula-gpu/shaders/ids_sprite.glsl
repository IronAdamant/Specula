// Sight's ID buffer, sprites and tile chunks (see src/idbuffer.rs): the fragments the scene
// shows (sprite.frag's cuts exactly: cutout items below their cutoff, other items at alpha 0,
// with the same texel), by MODE:
// 0 opaque: cutout items write their id (depth tested and written, as the scene's cutouts);
// 1 insert: blended items join their pixel's id set (depth tested, not written);
// 2 raster: every item marks its id rasterized at all (no depth test).
// Included by ids_sprite.frag (opaque) and ids_sprite_count.frag (COUNTING: insert, raster).
#extension GL_EXT_nonuniform_qualifier : require
#define IDS
#include "sprite.glsl"

#ifdef COUNTING
// Side effects only for fragments that pass the depth test (the opaque mode writes depth after
// its cut, so it keeps the usual late test).
layout(early_fragment_tests) in;
#endif

layout(constant_id = 0) const uint MODE = 0u;

layout(set = 0, binding = 1) uniform sampler2D textures[];

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec3 v_world;
layout(location = 2) flat in vec4 v_tint;
layout(location = 3) flat in uvec4 v_info;
layout(location = 4) flat in vec4 v_rect;
layout(location = 5) flat in vec4 v_params;
layout(location = 6) flat in vec4 v_tile;
layout(location = 7) flat in vec3 v_normal;
layout(location = 8) flat in uint v_id;

layout(location = 0) out uint out_id;

#include "sprite_texel.glsl"

void main() {
    if (v_world.x != v_world.x || v_normal.x != v_normal.x) {
        return; // never (NaN): keeps the interface matching the vertex stage's outputs
    }
    vec4 texel = v_info.w == KIND_TILES ? tile_texel() : sprite_texel();
    float a = clamp(texel.a * v_tint.a, 0.0, 1.0);
    bool cutout = (v_info.y & FLAG_CUTOUT) != 0u;
    if (cutout ? a < v_params.x : a <= 0.0) {
        discard; // the scene shows nothing here
    }
    if (v_id == 0u) {
        discard;
    }
    IdTarget t;
    t.stats = pc.idp.stats;
    t.seen = pc.idp.seen;
    t.width = pc.idp.width;
    t.epoch = pc.idp.epoch;
    if (MODE == 0u) {
        out_id = v_id;
    } else if (MODE == 1u) {
        id_insert(t, v_id, uvec2(gl_FragCoord.xy));
    } else {
        id_rasterized(t, v_id);
    }
}
