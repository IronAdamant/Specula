// The texel a sprite or tile chunk fragment shows (no lighting): shared by sprite.frag and
// ids_sprite.frag, so sight cuts exactly the texels the image cuts. Included after sprite.glsl,
// the `textures` array and the vertex stage's inputs; not compiled on its own.

// The sprite's texel at v_uv (texel units), kept inside the bounds the vertex stage set.
vec4 sprite_texel() {
    if (v_info.x == 0u) {
        return vec4(1.0);
    }
    vec2 t = v_uv;
    if ((v_info.y & FLAG_NEAREST) != 0u) {
        t = clamp(floor(t), v_rect.xy, v_rect.zw) + 0.5;
    } else {
        t = clamp(t, v_rect.xy, v_rect.zw);
    }
    return textureLod(textures[nonuniformEXT(v_info.x - 1u)], t * v_tile.zw, 0.0);
}

// The tile chunk's texel. Cells (RGBA8 in the cell texture): r + 256 g = tile (1-based, 0 =
// empty); b bits 0..2 = flip x, flip y, swap x/y (applied first), bits 3..7 = frames - 1;
// a = frame time in 1/60 s (0 = still). Frames are consecutive tiles.
vec4 tile_texel() {
    ivec2 first = ivec2(v_rect.xy);
    ivec2 last = max(ivec2(v_rect.zw) - 1, first);
    ivec2 cell = clamp(ivec2(floor(v_uv)), first, last);
    vec2 f = clamp(v_uv - vec2(cell), 0.0, 1.0);
    uvec4 b = uvec4(round(texelFetch(textures[nonuniformEXT(v_info.z - 1u)], cell, 0) * 255.0));
    uint id = b.r | (b.g << 8u);
    if (id == 0u) {
        return vec4(0.0);
    }
    uint frames = (b.b >> 3u) + 1u;
    if (frames > 1u && b.a > 0u) {
        id += uint(floor(max(pc.frame.camera_time.w, 0.0) * 60.0 / float(b.a))) % frames;
    }
    if ((b.b & 4u) != 0u) {
        f = f.yx;
    }
    if ((b.b & 1u) != 0u) {
        f.x = 1.0 - f.x;
    }
    if ((b.b & 2u) != 0u) {
        f.y = 1.0 - f.y;
    }
    vec2 tpx = v_tile.xy;
    uint cols = uint(v_params.w);
    uint k = id - 1u;
    vec2 origin = vec2(v_params.y) + vec2(float(k % cols), float(k / cols)) * (tpx + v_params.z);
    vec2 t = origin + f * tpx;
    if ((v_info.y & FLAG_NEAREST) != 0u) {
        t = clamp(floor(t), origin, origin + tpx - 1.0) + 0.5;
    } else {
        t = clamp(t, origin + 0.5, origin + tpx - 0.5);
    }
    return textureLod(textures[nonuniformEXT(v_info.x - 1u)], t * v_tile.zw, 0.0);
}

