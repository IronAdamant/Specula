// Slug glyph coverage: a glyph's quadratic curves evaluated per pixel (Eric Lengyel's algorithm,
// JCGT 2017; patent dedicated to the public domain, March 2026; our own implementation). A
// line-for-line port of `SlugGlyph::coverage` / `coverage_from_words` in
// crates/specula-text/src/slug.rs (the reference, tested against the exact-area rasteriser); the
// store's word layout is documented there. Included by sprite.frag (GLYPH pipelines) and
// ids_sprite.glsl after sprite.glsl; not compiled on its own.
#extension GL_EXT_buffer_reference_uvec2 : require

layout(buffer_reference, std430, buffer_reference_align = 4) readonly buffer GlyphWords {
    uint w[];
};

// Most curves one band is walked for (crates/specula-text MAX_BAND_CURVES): bounds the work per
// pixel whatever the store holds.
const uint GLYPH_MAX_CURVES = 512u;

// Word `i` of the store, clamped inside its `len` resident words (a bad offset reads garbage,
// never outside the buffer).
uint glyph_word(GlyphWords g, uint len, uint i) {
    return g.w[min(i, len - 1u)];
}

float glyph_float(GlyphWords g, uint len, uint i) {
    return uintBitsToFloat(glyph_word(g, len, i));
}

// Which roots a ray crosses (bit 0: root 1, downward; bit 1: root 2, upward).
uint glyph_eligible(float y1, float y2, float y3) {
    uint shift = (y1 > 0.0 ? 2u : 0u) + (y2 > 0.0 ? 4u : 0u) + (y3 > 0.0 ? 8u : 0u);
    return (0x2E74u >> shift) & 3u;
}

// The curve's crossings of the ray along +x at y = 0: their x (curve relative to the pixel).
vec2 glyph_roots(vec2 p1, vec2 p2, vec2 p3) {
    vec2 a = p1 - 2.0 * p2 + p3;
    vec2 b = p1 - p2;
    float t1;
    float t2;
    if (abs(a.y) < 1.0 / 65536.0) {
        float t = b.y != 0.0 ? p1.y * 0.5 / b.y : 0.0;
        t1 = t;
        t2 = t;
    } else {
        float ra = 1.0 / a.y;
        float d = sqrt(max(b.y * b.y - a.y * p1.y, 0.0));
        t1 = (b.y - d) * ra;
        t2 = (b.y + d) * ra;
    }
    return vec2((a.x * t1 - 2.0 * b.x) * t1 + p1.x, (a.x * t2 - 2.0 * b.x) * t2 + p1.x);
}

uint glyph_band(float v, float lo, float hi, uint n) {
    float span = hi - lo;
    if (n <= 1u || span <= 0.0) {
        return 0u;
    }
    float b = floor((v - lo) * float(n) / span);
    if (!(b >= 0.0)) {
        return 0u;
    }
    return min(uint(b), n - 1u);
}

// Coverage (0..1) at em position `p` of the glyph whose bands start at word `band_at`
// (`counts`: horizontal bands low 16 bits, vertical high), box `box`; `ppe`: pixels per em
// along x and y.
float glyph_coverage(uvec2 address, uint len, uint band_at, uint counts, vec4 box, vec2 p, vec2 ppe) {
    if (len == 0u) {
        return 0.0;
    }
    GlyphWords g = GlyphWords(address);
    uint nh = counts & 0xFFFFu;
    uint nv = counts >> 16u;
    if (nh == 0u || nv == 0u) {
        return 0.0;
    }
    float cov[2] = float[2](0.0, 0.0);
    float wgt[2] = float[2](0.0, 0.0);
    for (uint axis = 0u; axis < 2u; axis++) {
        uint band = axis == 0u ? glyph_band(p.y, box.y, box.w, nh) : nh + glyph_band(p.x, box.x, box.z, nv);
        float scale = axis == 0u ? ppe.x : ppe.y;
        float sign = axis == 0u ? 1.0 : -1.0;
        uint first = glyph_word(g, len, band_at + 2u * band);
        uint count = min(glyph_word(g, len, band_at + 2u * band + 1u), GLYPH_MAX_CURVES);
        for (uint k = 0u; k < count; k++) {
            uint at = glyph_word(g, len, first + k);
            vec2 p1 = vec2(glyph_float(g, len, at), glyph_float(g, len, at + 1u)) - p;
            vec2 p2 = vec2(glyph_float(g, len, at + 2u), glyph_float(g, len, at + 3u)) - p;
            vec2 p3 = vec2(glyph_float(g, len, at + 4u), glyph_float(g, len, at + 5u)) - p;
            if (axis == 1u) {
                // The vertical ray: the same problem with x and y swapped.
                p1 = p1.yx;
                p2 = p2.yx;
                p3 = p3.yx;
            }
            if (max(max(p1.x, p2.x), p3.x) * scale < -0.5) {
                break; // sorted by descending reach: every curve left is behind the pixel
            }
            uint code = glyph_eligible(p1.y, p2.y, p3.y);
            if (code != 0u) {
                vec2 r = glyph_roots(p1, p2, p3) * scale;
                if ((code & 1u) != 0u) {
                    cov[axis] += sign * clamp(r.x + 0.5, 0.0, 1.0);
                    wgt[axis] = max(wgt[axis], clamp(1.0 - abs(r.x) * 2.0, 0.0, 1.0));
                }
                if (code > 1u) {
                    cov[axis] -= sign * clamp(r.y + 0.5, 0.0, 1.0);
                    wgt[axis] = max(wgt[axis], clamp(1.0 - abs(r.y) * 2.0, 0.0, 1.0));
                }
            }
        }
    }
    float blended = abs(cov[0] * wgt[0] + cov[1] * wgt[1]) / max(wgt[0] + wgt[1], 1.0 / 65536.0);
    return clamp(max(blended, min(abs(cov[0]), abs(cov[1]))), 0.0, 1.0);
}

#ifndef GLYPH_VERTEX
// Pixels per em along the glyph's x and y at this fragment (v_uv in em units): the length of
// each em coordinate's screen gradient, so rotation and perspective keep edges one pixel soft.
vec2 glyph_pixels_per_em(vec2 em) {
    vec2 dx = dFdx(em);
    vec2 dy = dFdy(em);
    vec2 epp = sqrt(dx * dx + dy * dy);
    return 1.0 / max(epp, vec2(1.0 / 65536.0));
}
#endif
