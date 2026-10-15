#version 460
// Sprites and tile chunks, vertex stage: instance i is item i (four corners, drawn indexed),
// facing the camera as its billboard mode says. Everything the fragment stage needs travels as
// flat varyings (reading the record per fragment costs integrated GPUs dearly), and whatever
// depends only on the item (texture sizes, sampling bounds, tileset columns) is worked out here,
// once per corner instead of once per pixel.
// Compile with shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#include "sprite.glsl"

layout(set = 0, binding = 1) uniform sampler2D textures[];

layout(location = 0) out vec2 v_uv;          // sprite: texel coordinate | tiles: cell coordinate
layout(location = 1) out vec3 v_world;
layout(location = 2) flat out vec4 v_tint;
layout(location = 3) flat out uvec4 v_info;
layout(location = 4) flat out vec4 v_rect;   // sprite: texel bounds lo.xy, hi.xy | tiles: cells x0, y0, x1, y1
layout(location = 5) flat out vec4 v_params; // cutoff | tiles: margin, spacing (px), tileset columns
layout(location = 6) flat out vec4 v_tile;   // tiles: tile size (px) | both: 1 / texture size
layout(location = 7) flat out vec3 v_normal;

void main() {
    Item it = pc.items.items[gl_InstanceIndex];
    Corner c = item_corner(it, QUAD[gl_VertexIndex]);
    vec4 clip = pc.frame.view_proj * vec4(c.world, 1.0);
    if ((it.info.y & FLAG_SNAP) != 0u && ortho_view()) {
        clip = snap_to_pixels(clip, c.snap_anchor);
    }
    gl_Position = clip;
    v_world = c.world;
    v_tint = it.tint;
    v_info = it.info;
    v_normal = c.normal;
    vec2 size = vec2(1.0);
    if (it.info.x != 0u) {
        size = vec2(textureSize(textures[nonuniformEXT(it.info.x - 1u)], 0));
    }
    bool nearest = (it.info.y & FLAG_NEAREST) != 0u;
    if (it.info.w == KIND_TILES) {
        v_uv = c.uv;
        v_rect = vec4(it.v3.xy, it.v3.xy + it.v3.zw);
        vec2 tpx = max(it.v2.zw, vec2(1.0));
        float margin = it.v4.w;
        float spacing = it.v1.w;
        float cols = max(floor((size.x - 2.0 * margin + spacing) / (tpx.x + spacing)), 1.0);
        v_params = vec4(it.v5.w, margin, spacing, cols);
        v_tile = vec4(tpx, 1.0 / size);
    } else {
        // Texel units; the fragment stage clamps into the item's own rect, so atlas neighbours
        // never bleed in: whole texels for nearest sampling, half a texel inside for bilinear.
        v_uv = c.uv * size;
        vec2 lo = min(it.v3.xy, it.v3.zw) * size;
        vec2 hi = max(it.v3.xy, it.v3.zw) * size;
        if (nearest) {
            vec2 first = floor(lo + 1e-3);
            v_rect = vec4(first, max(ceil(hi - 1e-3) - 1.0, first));
        } else {
            vec2 a = lo + 0.5;
            v_rect = vec4(a, max(hi - 0.5, a));
        }
        v_params = vec4(it.v5.w, 0.0, 0.0, 0.0);
        v_tile = vec4(0.0, 0.0, 1.0 / size);
    }
}
