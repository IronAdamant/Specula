// Baked GI (docs/design/GI.md; src/gi.rs): irradiance from the resident probe volumes, the
// GPU twin of `specula_lux::Field::sample` (crates/specula-lux/src/field.rs). Included after the
// stage's push block (`pc.frame`) and its FEATURES constant; not compiled on its own.
//
// Buffer (16-byte words): [0] volume count, first lean probe; per volume three (origin xyz +
// spacing, dims xyz + first probe, intensity); then three per probe (`specula_lux::Record`: SH and
// offset/weight as half pairs, its cell's visibility mask and its fourteen reaches); then one per
// probe, its lean copy.
//
// Tiers (src/gi.rs `GiQuality`): with FEAT_GI_PIXEL (high) meshes sample the probes per pixel
// (`gi_ambient`); without it (low) small draws read an ambient cube the CPU sampled (`GiDraws`)
// and wide draws and terrain the lean probe copy (`gi_ambient_lean`: masks, no reaches). The full
// sampler's registers alone cost the Intel iGPU about 0.8 ms of Sepulcra's scene pass, run or
// not (its fragment shaders lost occupancy). Decals and particles sample per pixel.

const bool GI = (FEATURES & 512u) != 0u;
const bool GI_PIXEL = (FEATURES & 1024u) != 0u;

// The volume `p` (biased off its surface along `n`, towards `v`) samples: the first holding it,
// else the nearest within a spacing (it fades out). -1: none. `q` is the biased point.
int gi_volume(vec3 p, vec3 n, vec3 v, out vec3 q, out float out_by) {
    GiData g = pc.frame.gi_data;
    uint count = uint(pc.frame.gi_params.x);
    int best = -1;
    out_by = 1.0;
    q = p;
    [[dont_unroll]] for (uint i = 0u; i < count; i++) {
        vec4 os = uintBitsToFloat(g.words[1u + i * 3u]);
        uvec3 dims = g.words[2u + i * 3u].xyz;
        vec3 qi = p + n * (0.25 * os.w) + v * (0.05 * os.w);
        vec3 local = (qi - os.xyz) / os.w;
        vec3 o3 = max(-local, local - vec3(dims - 1u));
        float o = max(max(o3.x, o3.y), max(o3.z, 0.0));
        if (o < out_by) {
            best = int(i);
            out_by = o;
            q = qi;
            if (o <= 0.0) {
                break;
            }
        }
    }
    return best;
}

// Where `q` falls in a volume: its cell's lowest corner (probe index), the fraction across the
// cell, the cell's visibility bits for `q`'s octant, and the volume's frame.
struct GiCell {
    uint cell;
    uint layer;
    uint row;
    uint probes;
    vec3 f;
    vec3 base; // the cell's lowest probe minus q
    float s;
    uint bits;
    float intensity;
};

GiCell gi_cell(uint vi, vec3 q) {
    GiData g = pc.frame.gi_data;
    uint count = uint(pc.frame.gi_params.x);
    vec4 os = uintBitsToFloat(g.words[1u + vi * 3u]);
    uvec4 df = g.words[2u + vi * 3u];
    GiCell c;
    c.intensity = uintBitsToFloat(g.words[3u + vi * 3u].x);
    c.s = os.w;
    vec3 m = vec3(df.xyz - 1u);
    vec3 lc = clamp((q - os.xyz) / c.s, vec3(0.0), m);
    vec3 b = min(floor(lc), max(m - 1.0, vec3(0.0)));
    c.f = lc - b;
    uvec3 bi = uvec3(b);
    c.layer = df.x * df.z;
    c.row = df.x;
    c.cell = df.w + bi.y * c.layer + bi.z * df.x + bi.x;
    c.probes = 1u + count * 3u;
    c.base = os.xyz + b * c.s - q;
    uint octant = uint(c.f.x >= 0.5) | (uint(c.f.y >= 0.5) << 1) | (uint(c.f.z >= 0.5) << 2);
    c.bits = 0xffu;
    if (pc.frame.gi_params.y > 0.5) {
        uvec2 mask = g.words[c.probes + c.cell * 3u + 2u].xy;
        c.bits = ((octant < 4u ? mask.x : mask.y) >> ((octant & 3u) * 8u)) & 0xffu;
    }
    return c;
}

// Whether reach nibble `k` of `r` covers `along` (spacing `s`).
bool gi_reach(uvec2 r, uint k, float along, float s) {
    uint v = ((k < 8u ? r.x : r.y) >> ((k & 7u) * 4u)) & 15u;
    return v >= 15u || along <= (float(v) + 1.0) * s * 0.125;
}

// Whether a probe sees `d` (the point minus the probe) by its reaches (`r`: fourteen nibbles,
// eighths of the spacing `s`): no further along the nearest of its fourteen directions than it
// sees that way, nor along the second nearest when nearly as near. The twin of
// `specula_lux::field::Probe::reaches`.
bool gi_reaches(uvec2 r, vec3 d, float s) {
    vec3 a = abs(d);
    float face = max(a.x, max(a.y, a.z));
    float diag = (a.x + a.y + a.z) * 0.57735026;
    uint kf;
    if (a.x >= a.y && a.x >= a.z) {
        kf = d.x < 0.0 ? 1u : 0u;
    } else if (a.y >= a.z) {
        kf = d.y < 0.0 ? 3u : 2u;
    } else {
        kf = d.z < 0.0 ? 5u : 4u;
    }
    uint kd = 6u + (d.x < 0.0 ? 1u : 0u) + (d.y < 0.0 ? 2u : 0u) + (d.z < 0.0 ? 4u : 0u);
    bool f = gi_reach(r, kf, face, s);
    bool g = gi_reach(r, kd, diag, s);
    return face >= diag ? (f && (diag < face * 0.95 || g)) : (g && (face < diag * 0.95 || f));
}

// Corner `corner`'s share (trilinear x backface for normal `n` x validity; corners hidden by the
// cell's mask or their own reaches, or at no trilinear weight, skipped): its SH coefficients added
// to c0/cx/cy/cz, its weight to w; the trilinear weights of the valid corners and of the visible
// ones to `tw_valid` and `tw_seen`.
void gi_corner(GiCell c, uint corner, vec3 n, inout vec3 c0, inout vec3 cx, inout vec3 cy, inout vec3 cz, inout float w, inout float tw_valid, inout float tw_seen) {
    GiData g = pc.frame.gi_data;
    uvec3 o = uvec3(corner & 1u, (corner >> 1) & 1u, corner >> 2);
    uint at = c.probes + (c.cell + o.y * c.layer + o.z * c.row + o.x) * 3u;
    uvec4 wb = g.words[at + 1u];
    vec2 ow = unpackHalf2x16(wb.w);
    if (ow.y <= 0.0) {
        return; // left inside geometry
    }
    vec3 t3 = mix(1.0 - c.f, c.f, vec3(o));
    float tw = t3.x * t3.y * t3.z;
    tw_valid += tw;
    if (((c.bits >> corner) & 1u) == 0u || tw <= 0.0) {
        return; // hidden from the point's octant (a wall between), or on the cell's far face
    }
    vec3 to = c.base + vec3(o) * c.s + vec3(unpackHalf2x16(wb.z), ow.x);
    if (pc.frame.gi_params.y > 0.5 && !gi_reaches(g.words[at + 2u].zw, -to, c.s)) {
        return; // behind the first surface the probe sees that way
    }
    tw_seen += tw;
    uvec4 wa = g.words[at];
    float d2 = dot(to, to);
    float k = d2 > 1e-12 ? (dot(to, n) * inversesqrt(d2) + 1.0) * 0.5 : 1.0;
    float wt = max(tw * (k * k + 0.2), tw * 1e-4) * ow.y;
    vec2 h0 = unpackHalf2x16(wa.x);
    vec2 h1 = unpackHalf2x16(wa.y);
    vec2 h2 = unpackHalf2x16(wa.z);
    vec2 h3 = unpackHalf2x16(wa.w);
    vec2 h4 = unpackHalf2x16(wb.x);
    vec2 h5 = unpackHalf2x16(wb.y);
    c0 += wt * vec3(h0.x, h0.y, h1.x);
    cx += wt * vec3(h1.y, h2.x, h2.y);
    cy += wt * vec3(h3.x, h3.y, h4.x);
    cz += wt * vec3(h4.y, h5.x, h5.y);
    w += wt;
}

// The irradiance from summed corners for normal `n`, blended into the flat ambient by the
// volume's edge fade. Visible corners carrying under a quarter of the valid trilinear weight dim
// it in proportion (a far probe alone past a wall beside the point mustn't light it in full:
// `specula_lux::field::VISIBLE_SHARE`).
vec3 gi_resolve(vec3 c0, vec3 cx, vec3 cy, vec3 cz, float w, float tw_valid, float tw_seen, vec3 n, float intensity, float out_by) {
    float fade = clamp(1.0 - out_by, 0.0, 1.0);
    vec3 e = vec3(0.0);
    if (w > 0.0) {
        float dim = min(tw_seen / (tw_valid * 0.25), 1.0);
        e = max((c0 + cx * n.x + cy * n.y + cz * n.z) / w, vec3(0.0)) * (intensity * dim);
    } else if (tw_valid <= 0.0) {
        fade = 0.0; // every probe around is inside geometry: nothing known
    } // else: an enclosed pocket no probe sees into: dark
    return mix(pc.frame.ambient.rgb, e, fade);
}

// The surface's ambient irradiance at `p` facing `n` (unit, or zero for volumes such as smoke),
// seen from unit `v` towards the camera (or zero): the probes', blended into the flat ambient
// over a spacing at a volume's edge, or the flat ambient without GI.
vec3 gi_ambient(vec3 p, vec3 n, vec3 v) {
    if (!GI) {
        return pc.frame.ambient.rgb;
    }
    vec3 q;
    float out_by;
    int vi = gi_volume(p, n, v, q, out_by);
    if (vi < 0) {
        return pc.frame.ambient.rgb;
    }
    GiCell c = gi_cell(uint(vi), q);
    vec3 c0 = vec3(0.0), cx = vec3(0.0), cy = vec3(0.0), cz = vec3(0.0);
    float w = 0.0, tw_valid = 0.0, tw_seen = 0.0;
    [[dont_unroll]] for (uint corner = 0u; corner < 8u; corner++) {
        gi_corner(c, corner, n, c0, cx, cy, cz, w, tw_valid, tw_seen);
    }
    return gi_resolve(c0, cx, cy, cz, w, tw_valid, tw_seen, n, c.intensity, out_by);
}


// The lean probes' irradiance at `p` facing `n` (tier low's wide draws and terrain): the eight
// probes around in their lean copy (one word each: the mean and the direction's luminance,
// src/gi.rs `LeanProbe`), trilinear, with the cell's visibility mask and the probes' validity,
// but no reaches and no backface weights (the full sampler's registers cost the iGPU's every
// draw). Faded into the flat ambient at a volume's edge like `gi_ambient`.
vec3 gi_ambient_lean(vec3 p, vec3 n, vec3 v) {
    if (!GI) {
        return pc.frame.ambient.rgb;
    }
    vec3 q;
    float out_by;
    int vi = gi_volume(p, n, v, q, out_by);
    if (vi < 0) {
        return pc.frame.ambient.rgb;
    }
    GiCell c = gi_cell(uint(vi), q);
    GiData g = pc.frame.gi_data;
    uint lean = g.words[0].y;
    vec4 acc = vec4(0.0);
    vec3 dir = vec3(0.0);
    float tw_valid = 0.0, tw_seen = 0.0;
    [[unroll]] for (uint corner = 0u; corner < 8u; corner++) {
        uvec3 o = uvec3(corner & 1u, (corner >> 1) & 1u, corner >> 2);
        uvec4 s = g.words[lean + c.cell + o.y * c.layer + o.z * c.row + o.x];
        // Without branches: invalid probes (left inside geometry, weight 0) and probes hidden
        // from the point's octant (a wall between) weigh nothing.
        float valid = uintBitsToFloat(s.w);
        vec3 t3 = mix(1.0 - c.f, c.f, vec3(o));
        float tw = t3.x * t3.y * t3.z * (valid > 0.0 ? 1.0 : 0.0);
        float seen = tw * float((c.bits >> corner) & 1u);
        tw_valid += tw;
        tw_seen += seen;
        float wt = seen * valid;
        vec2 a0 = unpackHalf2x16(s.x), a1 = unpackHalf2x16(s.y), a2 = unpackHalf2x16(s.z);
        acc += vec4(vec3(a0, a1.x) * wt, wt);
        dir += vec3(a1.y, a2) * wt;
    }
    float fade = clamp(1.0 - out_by, 0.0, 1.0);
    vec3 e = vec3(0.0);
    if (acc.w > 0.0) {
        vec3 mean = acc.rgb / acc.w;
        const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);
        float dim = min(tw_seen / (tw_valid * 0.25), 1.0);
        e = max(mean * (1.0 + dot(dir / acc.w, n) / max(dot(mean, LUMA), 1e-4)), vec3(0.0)) * (c.intensity * dim);
    } else if (tw_valid <= 0.0) {
        fade = 0.0; // every probe around is inside geometry: nothing known
    }
    return mix(pc.frame.ambient.rgb, e, fade);
}

// A draw's ambient cube for normal `n` (src/gi.rs `sample_cube`): the irradiance facing +x, -x,
// +y, -y, +z, -z as rgb halves in nine words, each axis weighted by n squared.
vec3 gi_cube(uvec4 h, uvec4 h1, uint m, vec3 n) {
    vec2 a0 = unpackHalf2x16(h.x), a1 = unpackHalf2x16(h.y), a2 = unpackHalf2x16(h.z);
    vec2 a3 = unpackHalf2x16(h.w), a4 = unpackHalf2x16(h1.x), a5 = unpackHalf2x16(h1.y);
    vec2 a6 = unpackHalf2x16(h1.z), a7 = unpackHalf2x16(h1.w), a8 = unpackHalf2x16(m);
    vec3 n2 = n * n;
    vec3 x = n.x >= 0.0 ? vec3(a0, a1.x) : vec3(a1.y, a2);
    vec3 y = n.y >= 0.0 ? vec3(a3, a4.x) : vec3(a4.y, a5);
    vec3 z = n.z >= 0.0 ? vec3(a6, a7.x) : vec3(a7.y, a8);
    return x * n2.x + y * n2.y + z * n2.z;
}

// The ambient cube of draw record `rec` for normal `n` (tier low; water reads only this: the
// lean sampler's registers alone cost the iGPU a millisecond of a pond filling the view), else
// the flat ambient.
vec3 gi_cube_draw(uint rec, vec3 n) {
    if (GI && pc.frame.gi_params.w > 0.5) {
        GiDraws d = pc.frame.gi_draws;
        uvec4 m = d.d[rec * 3u + 2u];
        if (m.w == 1u) {
            return gi_cube(d.d[rec * 3u], d.d[rec * 3u + 1u], m.x, n);
        }
    }
    return pc.frame.ambient.rgb;
}

// The ambient for draw record `rec` at `p` facing `n`: at tier low its ambient cube (three loads)
// or, for a wide draw, the lean probes per pixel; at high the probes per pixel (FEAT_GI_PIXEL);
// else the flat ambient.
vec3 gi_ambient_draw(uint rec, vec3 p, vec3 n, vec3 v) {
    if (!GI) {
        return pc.frame.ambient.rgb;
    }
    if (pc.frame.gi_params.w > 0.5) {
        // The record's three words at once (independent loads: one memory latency, not three).
        GiDraws d = pc.frame.gi_draws;
        uvec4 h = d.d[rec * 3u];
        uvec4 h1 = d.d[rec * 3u + 1u];
        uvec4 m = d.d[rec * 3u + 2u];
        if (m.w == 1u) {
            return gi_cube(h, h1, m.x, n);
        }
        if (!GI_PIXEL && m.w == 3u) { // compiled out of the per-pixel pipelines
            return gi_ambient_lean(p, n, v);
        }
    }
    if (GI_PIXEL) {
        return gi_ambient(p, n, v);
    }
    return pc.frame.ambient.rgb;
}

// How much of the surroundings' reflection a surface keeps: its GI against the flat ambient
// (a cheap specular occlusion: inside the crypt, rough metal no longer mirrors the night sky).
float gi_occlusion(vec3 ambient) {
    if (!GI) {
        return 1.0;
    }
    const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);
    return clamp(dot(ambient, LUMA) / max(dot(pc.frame.ambient.rgb, LUMA), 1e-4), 0.0, 1.0);
}

// The irradiance debug view (sight_frame gi: "irradiance"): show the ambient term alone.
bool gi_irradiance_view() {
    return GI && pc.frame.gi_params.z > 0.5;
}
