// Specula terrain (src/terrain.rs, docs/design/TERRAIN.md): the material records, the bindless
// textures and the CDLOD patch vertex, shared by terrain.vert, terrain_shadow.vert,
// ids_terrain.vert and terrain.frag. Included after frame.glsl; not compiled on its own.
#extension GL_EXT_nonuniform_qualifier : require

// One splat layer; must match `LayerRecord` in src/terrain.rs.
struct TerrainLayer {
    vec4 tint_rough; // linear tint, roughness
    vec4 params;     // specular, 1 / uv scale, cos(triplanar slope) (-1: never), normal strength
    uvec4 tex;       // albedo slot + 1, normal slot + 1 (0 = none)
};

// One terrain; must match `TerrainRecord` in src/terrain.rs (256 bytes).
struct Terrain {
    vec4 grid;  // x0, z0 (local position of sample 0), cell x, cell z
    uvec4 size; // columns, rows, heights slot + 1, normals slot + 1
    uvec4 more; // splat slot + 1, layer count
    vec4 look;  // grain
    TerrainLayer layers[4];
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Terrains {
    Terrain items[];
};

// The bindless texture array (descriptors.rs).
layout(set = 0, binding = 1) uniform sampler2D textures[];

// A sample's height: the f32's four bytes in an RGBA8 texel, reassembled bit for bit.
float terrain_height(uint slot, ivec2 p) {
    vec4 t = texelFetch(textures[nonuniformEXT(slot - 1u)], p, 0);
    uvec4 b = uvec4(round(t * 255.0));
    return uintBitsToFloat(b.r | (b.g << 8) | (b.b << 16) | (b.a << 24));
}

// A patch vertex: its world position, its (morphed) sample coordinates and whether it lies on a
// skirt (src/terrain.rs `patch_vertex` mirrors this; keep them in step).
//   Patch record: color = (i0, j0, step, quarter), color2 = (r0: level 0's range end, the
//   data's level offset (3 for a coarse copy), skirt depth, 0) (each level's morph band follows from
//   r0: `morph_range` in src/terrain.rs),
//   ext = (terrain, skirt sides: 1 -x, 2 +x, 4 -z, 8 +z, level, root level); model = local -> world.
//   Grid vertex: position = (x, ring, z) with x, z in 0..=n (16, or 8 for a quarter).
struct TerrainVertex {
    vec3 world;
    vec2 cell;
};

TerrainVertex terrain_vertex(Frame f, Draw d, Terrains ts, vec3 in_pos) {
    Terrain t = ts.items[d.ext.x];
    vec2 grid = in_pos.xz;
    bool ring = in_pos.y > 0.5;
    float step = d.color.z;
    float n = d.color.w > 0.5 ? 8.0 : 16.0;
    vec2 size = vec2(t.size.xy) - 1.0;
    vec2 base = d.color.xy + grid * step;
    uint hs = t.size.z;
    // Collapse onto each coarser level's grid in turn (data levels; the morph ranges are the full
    // terrain's: r0 · 2^(level + offset), morphing over the last 30% of each band). Each step's
    // distance is measured at the vertex as it stands on that level's grid, a vertex the coarser
    // patch has too, so both sides of a border agree step by step.
    float r0 = d.color2.x;
    float offset = d.color2.y;
    vec2 q = base;
    // Never past the data's root level (ext.w): the root patch is the coarsest it gets.
    for (uint k = d.ext.z; k < min(d.ext.w, 12u); k++) {
        vec2 c = min(q, size);
        float h = terrain_height(hs, ivec2(c));
        vec4 w = d.model * vec4(t.grid.x + c.x * t.grid.z, h, t.grid.y + c.y * t.grid.w, 1.0);
        float dist = length(w.xyz - f.camera_time.xyz);
        float full = float(k) + offset;
        float end = r0 * exp2(full);
        float start = full == 0.0 ? 0.7 * end : 0.85 * end;
        float m = clamp((dist - start) / max(end - start, 1e-6), 0.0, 1.0);
        if (m <= 0.0) {
            break;
        }
        float span = exp2(float(k + 1u));
        q = mix(q, q - mod(q, vec2(span)), m);
        if (m < 1.0) {
            break;
        }
    }
    vec2 mp = min(q, size);
    vec2 fl = floor(mp);
    vec2 fr = mp - fl;
    float hm = terrain_height(hs, ivec2(fl));
    if (fr.x != 0.0 || fr.y != 0.0) {
        // Between samples only while morphing: bilinear.
        vec2 f1 = min(fl + 1.0, size);
        float h10 = terrain_height(hs, ivec2(f1.x, fl.y));
        float h01 = terrain_height(hs, ivec2(fl.x, f1.y));
        float h11 = terrain_height(hs, ivec2(f1));
        hm = mix(mix(hm, h10, fr.x), mix(h01, h11, fr.x), fr.y);
    }
    if (ring) {
        // The skirt hangs from the terrain's border sides its mask names; elsewhere the ring
        // folds onto the border (degenerate triangles).
        uint sides = d.ext.y;
        bool drop = (grid.x == 0.0 && base.x <= 0.0 && (sides & 1u) != 0u) || (grid.x == n && base.x >= size.x && (sides & 2u) != 0u) ||
                    (grid.y == 0.0 && base.y <= 0.0 && (sides & 4u) != 0u) || (grid.y == n && base.y >= size.y && (sides & 8u) != 0u);
        if (drop) {
            hm -= d.color2.z;
        }
    }
    TerrainVertex o;
    o.world = (d.model * vec4(t.grid.x + mp.x * t.grid.z, hm, t.grid.y + mp.y * t.grid.w, 1.0)).xyz;
    o.cell = mp;
    return o;
}
