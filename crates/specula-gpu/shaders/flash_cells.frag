#version 460
// The flash watch's blocks: one texel per 8x8 block of the final image: its mean relative
// luminance and red value, and the share of its pixels whose state made a transition this frame,
// all from the 4x4 state texels it covers (src; flash.glsl), each weighted by the pixels it
// covers (partial blocks at odd edges count only their own pixels). params: the final image's
// width and height. Compile with shaders/build.sh.
#include "post.glsl"
#include "flash.glsl"

void main() {
    ivec2 b = ivec2(gl_FragCoord.xy);
    ivec2 size = ivec2(pc.params.xy);
    vec2 state = vec2(textureSize(textures[pc.src], 0));
    // textureGather's order within a 2x2 quad.
    const ivec2 order[4] = ivec2[4](ivec2(0, 1), ivec2(1, 1), ivec2(1, 0), ivec2(0, 0));
    vec4 acc = vec4(0.0);
    float w = 0.0;
    for (int qy = 0; qy < 2; qy++) {
        for (int qx = 0; qx < 2; qx++) {
            ivec2 h0 = b * 4 + ivec2(qx, qy) * 2;
            // Texels beyond the state clamp onto its edge; they cover no pixels, so weigh nothing.
            vec2 uv = vec2(h0 + 1) / state;
            vec4 g = textureGather(textures[pc.src], uv, 1);
            vec4 a = textureGather(textures[pc.src], uv, 3);
            for (int k = 0; k < 4; k++) {
                ivec2 px = (h0 + order[k]) * 2;
                if (px.x < size.x && px.y < size.y) {
                    float count = float(min(2, size.x - px.x) * min(2, size.y - px.y));
                    ivec2 l = unpack_state(g[k]);
                    ivec2 r = unpack_state(a[k]);
                    acc += count * vec4(float(l.y) / 255.0, float(r.y) / 255.0, lum_moved(l.x) ? 1.0 : 0.0, red_moved(r.x) ? 1.0 : 0.0);
                    w += count;
                }
            }
        }
    }
    out_color = acc / max(w, 1.0);
}
