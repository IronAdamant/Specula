// Shared by the 2D stages (sprites, tile chunks, vector shapes, sprite shadows). Included; not
// compiled on its own.
#include "frame.glsl"

// One 2D item; must match `ItemRecord` in src/sprite.rs (std430, 112 bytes).
//   sprite: v1 = position.xyz, rotation (rad) | v2 = size.xy, pivot.xy | v3 = uv rect u0 v0 u1 v1
//           v4 = axis x (right).xyz, unused   | v5 = axis y (up).xyz, alpha cutoff
//   tiles:  v1 = origin.xyz (top-left corner of cell 0,0), spacing (px)
//           v2 = tile size in metres xy, tile size in tileset pixels xy
//           v3 = drawn cells x0, y0, w, h     | v4 = column axis.xyz, margin (px)
//           v5 = row axis.xyz (rows go this way), alpha cutoff
//   shape:  v1..v4 = model matrix columns     | v5 unused
// info: texture slot + 1 (0 = none), flags (FLAG_*), tiles: cell texture slot + 1, kind (KIND_*).
struct Item {
    vec4 tint; // linear RGB, straight alpha
    vec4 v1;
    vec4 v2;
    vec4 v3;
    vec4 v4;
    vec4 v5;
    uvec4 info;
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Items {
    Item items[];
};

const uint KIND_SPRITE = 0u;
const uint KIND_TILES = 1u;
const uint KIND_SHAPE = 2u;

const uint FLAG_BILLBOARD_MASK = 3u; // 0 none, 1 spherical, 2 cylindrical
const uint FLAG_FLIP_X = 4u;
const uint FLAG_FLIP_Y = 8u;
const uint FLAG_NEAREST = 16u;
const uint FLAG_SNAP = 32u;
const uint FLAG_LIT = 64u;
const uint FLAG_CUTOUT = 128u;

// Must match `SpritePush` in src/sprite.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Items items;
    uint pass;     // shadow cascade (sprite shadows)
    uint mode;     // bit 0: orthographic view
    vec4 cam_pos;  // xyz: camera position (orthographic: centre of the near plane)
    vec4 cam_right;
    vec4 cam_up;
    vec4 cam_forward;
} pc;

bool ortho_view() { return (pc.mode & 1u) != 0u; }

// A sprite's or tile chunk's corner: world position, texture coordinate (tiles: cell coordinate)
// and the world position of corner (0, 0) (what pixel snapping aligns).
struct Corner {
    vec3 world;
    vec2 uv;
    vec3 snap_anchor;
    vec3 normal;
};

// Corner `q` (0..1 each way, (0, 0) = bottom left of a sprite, top left of a tile chunk).
Corner item_corner(Item it, vec2 q) {
    Corner c;
    if (it.info.w == KIND_TILES) {
        vec3 ax = it.v4.xyz;
        vec3 ay = it.v5.xyz;
        vec2 cell = it.v3.xy + q * it.v3.zw;
        c.world = it.v1.xyz + ax * (cell.x * it.v2.x) + ay * (cell.y * it.v2.y);
        c.snap_anchor = it.v1.xyz + ax * (it.v3.x * it.v2.x) + ay * (it.v3.y * it.v2.y);
        c.uv = cell;
        c.normal = cross(ay, ax);
        return c;
    }
    uint flags = it.info.y;
    uint billboard = flags & FLAG_BILLBOARD_MASK;
    vec3 pos = it.v1.xyz;
    vec3 right = it.v4.xyz;
    vec3 up = it.v5.xyz;
    if (billboard == 1u) {
        right = pc.cam_right.xyz;
        up = pc.cam_up.xyz;
    } else if (billboard == 2u) {
        vec3 to_cam = ortho_view() ? -pc.cam_forward.xyz : pc.cam_pos.xyz - pos;
        vec3 r = cross(up, to_cam);
        right = dot(r, r) > 1e-12 ? normalize(r) : pc.cam_right.xyz;
    }
    vec2 size = it.v2.xy;
    vec2 pivot = it.v2.zw;
    float s = sin(it.v1.w);
    float k = cos(it.v1.w);
    vec2 o = (q - pivot) * size;
    o = vec2(k * o.x - s * o.y, s * o.x + k * o.y);
    vec2 o0 = -pivot * size;
    o0 = vec2(k * o0.x - s * o0.y, s * o0.x + k * o0.y);
    c.world = pos + right * o.x + up * o.y;
    c.snap_anchor = pos + right * o0.x + up * o0.y;
    float u = (flags & FLAG_FLIP_X) != 0u ? 1.0 - q.x : q.x;
    float v = (flags & FLAG_FLIP_Y) != 0u ? 1.0 - q.y : q.y;
    // Texture rows run top to bottom: the top of the sprite (q.y = 1) shows v0.
    c.uv = vec2(mix(it.v3.x, it.v3.z, u), mix(it.v3.w, it.v3.y, v));
    c.normal = cross(right, up);
    return c;
}

// Moves a clip position so that `anchor` lands on a whole pixel (orthographic views only).
vec4 snap_to_pixels(vec4 clip, vec3 anchor) {
    vec4 a = pc.frame.view_proj * vec4(anchor, 1.0);
    vec2 px = (a.xy / a.w * 0.5 + 0.5) * pc.frame.viewport.xy;
    vec2 d = round(px) - px;
    clip.xy += d * 2.0 / pc.frame.viewport.xy * clip.w;
    return clip;
}

// The six corners of a quad (two triangles).
const vec2 QUAD[6] = vec2[](vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0));
