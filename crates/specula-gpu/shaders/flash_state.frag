#version 460
// The flash watch's state at half resolution: each texel's 2x2 block of the final image (src,
// sRGB: decoded to linear before filtering), its extremes moved on from the previous state (src2),
// its means and whether it made a transition. params: the final image's width and height, and 1
// to start again (a new or resized watch). Compile with shaders/build.sh.
#include "post.glsl"
#include "flash.glsl"

void main() {
    ivec2 t = ivec2(gl_FragCoord.xy);
    vec2 size = pc.params.xy;
    // One bilinear tap at the block's shared corner: the exact mean of its 2x2 linear colours
    // (weights of a half each way). A partial block at an odd edge clamps onto its own pixels,
    // so it averages only those.
    vec3 c = textureLod(textures[pc.src], (vec2(t * 2) + 1.0) / size, 0.0).rgb;
    vec2 m = measure(c);
    vec4 fresh = vec4(m.x, pack_state(0, m.x), m.y, pack_state(1, m.y));
    // Starting again: the previous state isn't read (it holds nothing yet).
    if (pc.params.z > 0.5) {
        out_color = fresh;
        return;
    }
    vec4 prev = texelFetch(textures[pc.src2], t, 0);
    int lc = unpack_state(prev.g).x;
    int rc = unpack_state(prev.a).x;
    if (!red_started(rc)) {
        out_color = fresh;
        return;
    }
    float le = prev.r, ld = lum_dir(lc), re = prev.b, rd = red_dir(rc);
    float ld0 = ld, rd0 = rd;
    step_lum(le, ld, m.x);
    step_red(re, rd, m.y);
    out_color = vec4(le, pack_state(lum_code(ld, ld != ld0), m.x), re, pack_state(red_code(rd, rd != rd0), m.y));
}
