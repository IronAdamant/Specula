#version 460
// FXAA 3.11 (Timothy Lottes), the PC "quality" algorithm with a 6-step edge search, on the tone
// mapped image (src, an sRGB texture: samples are linear; luma is taken perceptually).
// Compile with shaders/build.sh.
#include "post.glsl"

const float EDGE_THRESHOLD = 0.125;
const float EDGE_THRESHOLD_MIN = 0.0312;
const float SUBPIX = 0.75;

vec3 rgb_at(vec2 uv) { return textureLod(textures[pc.src], uv, 0.0).rgb; }
float luma(vec3 c) { return sqrt(dot(c, vec3(0.299, 0.587, 0.114))); }
float luma_at(vec2 uv) { return luma(rgb_at(uv)); }
float luma_off(vec2 uv, vec2 o) { return luma_at(uv + o * pc.texel); }

void main() {
    vec2 uv = v_uv;
    vec2 rcp = pc.texel;
    vec3 rgb_m = rgb_at(uv);
    float luma_m = luma(rgb_m);
    float luma_s = luma_off(uv, vec2(0.0, 1.0));
    float luma_e = luma_off(uv, vec2(1.0, 0.0));
    float luma_n = luma_off(uv, vec2(0.0, -1.0));
    float luma_w = luma_off(uv, vec2(-1.0, 0.0));
    float range_max = max(max(max(luma_n, luma_w), max(luma_e, luma_s)), luma_m);
    float range_min = min(min(min(luma_n, luma_w), min(luma_e, luma_s)), luma_m);
    float range = range_max - range_min;
    if (range < max(EDGE_THRESHOLD_MIN, range_max * EDGE_THRESHOLD)) {
        out_color = vec4(rgb_m, 1.0);
        return;
    }
    float luma_nw = luma_off(uv, vec2(-1.0, -1.0));
    float luma_se = luma_off(uv, vec2(1.0, 1.0));
    float luma_ne = luma_off(uv, vec2(1.0, -1.0));
    float luma_sw = luma_off(uv, vec2(-1.0, 1.0));

    float luma_ns = luma_n + luma_s;
    float luma_we = luma_w + luma_e;
    float subpix_nswe = luma_ns + luma_we;
    float edge_horz1 = -2.0 * luma_m + luma_ns;
    float edge_vert1 = -2.0 * luma_m + luma_we;
    float luma_nese = luma_ne + luma_se;
    float luma_nwne = luma_nw + luma_ne;
    float edge_horz2 = -2.0 * luma_e + luma_nese;
    float edge_vert2 = -2.0 * luma_n + luma_nwne;
    float luma_nwsw = luma_nw + luma_sw;
    float luma_swse = luma_sw + luma_se;
    float edge_horz4 = abs(edge_horz1) * 2.0 + abs(edge_horz2);
    float edge_vert4 = abs(edge_vert1) * 2.0 + abs(edge_vert2);
    float edge_horz3 = -2.0 * luma_w + luma_nwsw;
    float edge_vert3 = -2.0 * luma_s + luma_swse;
    float edge_horz = abs(edge_horz3) + edge_horz4;
    float edge_vert = abs(edge_vert3) + edge_vert4;
    float subpix_nwswnese = luma_nwsw + luma_nese;
    bool horz_span = edge_horz >= edge_vert;
    float length_sign = horz_span ? rcp.y : rcp.x;
    if (!horz_span) {
        luma_n = luma_w;
        luma_s = luma_e;
    }
    float subpix_a = subpix_nswe * 2.0 + subpix_nwswnese;
    float subpix_b = subpix_a * (1.0 / 12.0) - luma_m;
    float gradient_n = luma_n - luma_m;
    float gradient_s = luma_s - luma_m;
    float luma_nn = luma_n + luma_m;
    float luma_ss = luma_s + luma_m;
    bool pair_n = abs(gradient_n) >= abs(gradient_s);
    float gradient = max(abs(gradient_n), abs(gradient_s));
    if (pair_n) {
        length_sign = -length_sign;
    }
    float subpix_c = clamp(abs(subpix_b) / range, 0.0, 1.0);

    vec2 pos_b = uv;
    vec2 off_np = horz_span ? vec2(rcp.x, 0.0) : vec2(0.0, rcp.y);
    if (horz_span) {
        pos_b.y += length_sign * 0.5;
    } else {
        pos_b.x += length_sign * 0.5;
    }
    vec2 pos_n = pos_b - off_np;
    vec2 pos_p = pos_b + off_np;
    float subpix_d = -2.0 * subpix_c + 3.0;
    float subpix_e = subpix_c * subpix_c;
    if (!pair_n) {
        luma_nn = luma_ss;
    }
    float gradient_scaled = gradient * 0.25;
    float luma_mm = luma_m - luma_nn * 0.5;
    float subpix_f = subpix_d * subpix_e;
    bool luma_m_lt_zero = luma_mm < 0.0;
    float luma_end_n = luma_at(pos_n) - luma_nn * 0.5;
    float luma_end_p = luma_at(pos_p) - luma_nn * 0.5;
    bool done_n = abs(luma_end_n) >= gradient_scaled;
    bool done_p = abs(luma_end_p) >= gradient_scaled;
    if (!done_n) pos_n -= off_np * 1.5;
    if (!done_p) pos_p += off_np * 1.5;
    const float STEPS[6] = float[](2.0, 2.0, 2.0, 2.0, 4.0, 8.0);
    for (int i = 0; i < 6 && !(done_n && done_p); i++) {
        if (!done_n) luma_end_n = luma_at(pos_n) - luma_nn * 0.5;
        if (!done_p) luma_end_p = luma_at(pos_p) - luma_nn * 0.5;
        done_n = done_n || abs(luma_end_n) >= gradient_scaled;
        done_p = done_p || abs(luma_end_p) >= gradient_scaled;
        if (!done_n) pos_n -= off_np * STEPS[i];
        if (!done_p) pos_p += off_np * STEPS[i];
    }
    float dst_n = horz_span ? uv.x - pos_n.x : uv.y - pos_n.y;
    float dst_p = horz_span ? pos_p.x - uv.x : pos_p.y - uv.y;
    bool good_span_n = (luma_end_n < 0.0) != luma_m_lt_zero;
    bool good_span_p = (luma_end_p < 0.0) != luma_m_lt_zero;
    float span_length = dst_p + dst_n;
    bool direction_n = dst_n < dst_p;
    float dst = min(dst_n, dst_p);
    bool good_span = direction_n ? good_span_n : good_span_p;
    float subpix_g = subpix_f * subpix_f;
    float pixel_offset = -dst / span_length + 0.5;
    float subpix_h = subpix_g * SUBPIX;
    float pixel_offset_good = good_span ? pixel_offset : 0.0;
    float pixel_offset_subpix = max(pixel_offset_good, subpix_h);
    vec2 p = uv;
    if (horz_span) {
        p.y += pixel_offset_subpix * length_sign;
    } else {
        p.x += pixel_offset_subpix * length_sign;
    }
    out_color = vec4(rgb_at(p), 1.0);
}
