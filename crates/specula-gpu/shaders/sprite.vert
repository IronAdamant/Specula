#version 460
// Sprites and tile chunks, vertex stage: instance i is item i (six vertices, a quad), facing the
// camera as its billboard mode says. Everything the fragment stage needs travels as flat
// varyings (reading the record per fragment costs integrated GPUs dearly).
// Compile with shaders/build.sh.
#include "sprite.glsl"

layout(location = 0) out vec2 v_uv;          // sprite: texture uv | tiles: cell coordinate
layout(location = 1) out vec3 v_world;
layout(location = 2) flat out vec4 v_tint;
layout(location = 3) flat out uvec4 v_info;
layout(location = 4) flat out vec4 v_rect;   // sprite: uv rect | tiles: cells x0, y0, x1, y1
layout(location = 5) flat out vec4 v_params; // cutoff, tiles: margin, spacing (px)
layout(location = 6) flat out vec4 v_tile;   // tiles: tile size in tileset pixels
layout(location = 7) flat out vec3 v_normal;

void main() {
    Item it = pc.items.items[gl_InstanceIndex];
    Corner c = item_corner(it, QUAD[gl_VertexIndex]);
    vec4 clip = pc.frame.view_proj * vec4(c.world, 1.0);
    if ((it.info.y & FLAG_SNAP) != 0u && ortho_view()) {
        clip = snap_to_pixels(clip, c.snap_anchor);
    }
    gl_Position = clip;
    v_uv = c.uv;
    v_world = c.world;
    v_tint = it.tint;
    v_info = it.info;
    v_normal = c.normal;
    if (it.info.w == KIND_TILES) {
        v_rect = vec4(it.v3.xy, it.v3.xy + it.v3.zw);
        v_params = vec4(it.v5.w, it.v4.w, it.v1.w, 0.0);
        v_tile = vec4(it.v2.zw, 0.0, 0.0);
    } else {
        v_rect = it.v3;
        v_params = vec4(it.v5.w, 0.0, 0.0, 0.0);
        v_tile = vec4(0.0);
    }
}
