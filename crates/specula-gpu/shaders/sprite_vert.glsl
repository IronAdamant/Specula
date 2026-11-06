// Included by sprite.vert and ids_sprite.vert (IDS: sight's ID buffer, the item's id too).
// Sprites and tile chunks, vertex stage: instance i is item i (four corners, drawn indexed),
// facing the camera as its billboard mode says. Everything the fragment stage needs travels as
// flat varyings (reading the record per fragment costs integrated GPUs dearly), and whatever
// depends only on the item (texture sizes, sampling bounds, tileset columns) is worked out here,
// once per corner instead of once per pixel.
// Compile with shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#include "sprite.glsl"
#define GLYPH_VERTEX
#include "glyph.glsl"

layout(set = 0, binding = 1) uniform sampler2D textures[];

layout(location = 0) out vec2 v_uv;          // sprite: texel coordinate | tiles: cell coordinate
layout(location = 1) out vec3 v_world;
layout(location = 2) flat out vec4 v_tint;
layout(location = 3) flat out uvec4 v_info;
layout(location = 4) flat out vec4 v_rect;   // sprite: texel bounds lo.xy, hi.xy | tiles: cells x0, y0, x1, y1
layout(location = 5) flat out vec4 v_params; // cutoff | tiles: margin, spacing (px), tileset columns
layout(location = 6) flat out vec4 v_tile;   // tiles: tile size (px) | both: 1 / texture size
layout(location = 7) flat out vec3 v_normal;
#ifdef IDS
layout(location = 8) flat out uint v_id; // sight's id of the item's entity (0: none)
#endif
// Glyphs: the store's address (xy), its resident words, the glyph's band counts.
layout(location = 9) flat out uvec4 v_glyph;

// A glyph (KIND_GLYPH; records from `glyph_record` in src/sprite.rs): its em box, grown by about a
// pixel on each axis so the anti-aliased edge has room (at most half an em), placed by its
// origin and per-em axes. v_uv carries em coordinates for the Slug coverage (glyph.glsl).
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer ItemRows {
    uvec4 rows[];
};

void glyph_vertex(Item it) {
    vec3 o = it.v1.xyz;
    vec3 ax = it.v2.xyz;
    vec3 ay = it.v3.xyz;
    vec4 box = vec4(it.v2.w, it.v3.w, it.v4.x, it.v4.y);
    // Pixels per em along each axis where the glyph sits.
    vec4 c0 = pc.frame.view_proj * vec4(o, 1.0);
    vec4 cx = pc.frame.view_proj * vec4(o + ax, 1.0);
    vec4 cy = pc.frame.view_proj * vec4(o + ay, 1.0);
    vec2 dilate = vec2(0.05);
    if (c0.w > 1e-6 && cx.w > 1e-6 && cy.w > 1e-6) {
        vec2 half_vp = 0.5 * pc.frame.viewport.xy;
        vec2 s0 = c0.xy / c0.w * half_vp;
        float px = length(cx.xy / cx.w * half_vp - s0);
        float py = length(cy.xy / cy.w * half_vp - s0);
        dilate = min(vec2(1.0 / max(px, 1e-3), 1.0 / max(py, 1e-3)), vec2(0.5));
    }
    vec2 q = QUAD[gl_VertexIndex];
    vec2 em = mix(box.xy - dilate, box.zw + dilate, q);
    vec3 world = o + ax * em.x + ay * em.y;
    gl_Position = pc.frame.view_proj * vec4(world, 1.0);
#ifdef IDS
    v_id = pc.idp.ids.items[gl_InstanceIndex];
#endif
    // The address's bits read as integers (an Item is seven 16-byte rows; v5 is row 5).
    uvec2 address = ItemRows(pc.items).rows[gl_InstanceIndex * 7 + 5].zw;
    uint len = it.info.z;
    uint word = it.info.x;
    GlyphWords g = GlyphWords(address);
    uint counts = len > 0u ? glyph_word(g, len, word + 4u) : 0u;
    uint band_at = len > 0u ? glyph_word(g, len, word + 5u) : 0u;
    v_uv = em;
    v_world = world;
    v_tint = it.tint;
    v_info = uvec4(band_at, it.info.y, len, KIND_GLYPH);
    v_rect = box;
    v_params = vec4(0.0);
    v_tile = vec4(0.0);
    v_normal = cross(ax, ay);
    v_glyph = uvec4(address, len, counts);
}

void main() {
    Item it = pc.items.items[gl_InstanceIndex];
    v_glyph = uvec4(0u);
    if (it.info.w == KIND_GLYPH) {
        glyph_vertex(it);
        return;
    }
    Corner c = item_corner(it, QUAD[gl_VertexIndex]);
    vec4 clip = pc.frame.view_proj * vec4(c.world, 1.0);
    if ((it.info.y & FLAG_SNAP) != 0u && ortho_view()) {
        clip = snap_to_pixels(clip, c.snap_anchor);
    }
    gl_Position = clip;
#ifdef IDS
    v_id = pc.idp.ids.items[gl_InstanceIndex];
#endif
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
