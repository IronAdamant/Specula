#version 460
// Overlay, vertex stage: instance i is record i, six vertices each.
// - Quads grow by their feather (and a pixel) on every side so the fragment stage can fade
//   fractional, rounded and soft edges, then shrink to their clip rect (no wasted fragments).
// - Triangles use vertices 0..2; vertices 3..5 collapse onto vertex 0 (a degenerate triangle).
// Everything the fragment stage needs travels as flat varyings: reading the record per
// fragment costs integrated GPUs dearly.
// Compile with shaders/build.sh.
#include "overlay.glsl"

layout(location = 0) flat out uvec2 v_info;    // texture slot + 1, kind
layout(location = 1) out vec2 v_uv;
layout(location = 2) out vec4 v_vcolor;        // triangles: the interpolated vertex colour
layout(location = 3) flat out vec4 v_rect;
layout(location = 4) flat out vec4 v_color;
layout(location = 5) flat out vec4 v_color2;
layout(location = 6) flat out vec4 v_border0;
layout(location = 7) flat out vec4 v_border1;
layout(location = 8) flat out vec4 v_params;
layout(location = 9) flat out vec4 v_clip;
layout(location = 10) flat out vec4 v_uv_rect;

vec4 to_clip(vec2 px) {
    return vec4(px / pc.frame.viewport.xy * 2.0 - 1.0, 0.0, 1.0);
}

void main() {
    Quad q = pc.quads.items[gl_InstanceIndex];
    v_info = q.info.xy;
    v_clip = q.clip;
    v_rect = q.rect;
    v_color = q.color;
    v_color2 = q.color2;
    v_border0 = q.border0;
    v_border1 = q.border1;
    v_params = q.params;
    v_uv_rect = q.uv;
    if (q.info.y == KIND_TRIANGLE) {
        uint k = gl_VertexIndex < 3 ? uint(gl_VertexIndex) : 0u;
        vec2 p = k == 0u ? q.rect.xy : (k == 1u ? q.rect.zw : q.uv.xy);
        v_vcolor = k == 0u ? q.color : (k == 1u ? q.color2 : q.border0);
        v_uv = vec2(0.0);
        gl_Position = to_clip(p);
        return;
    }
    const vec2 corners[6] = vec2[](vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0));
    vec2 size = max(q.rect.zw, vec2(0.0));
    float grow = ceil(0.5 * max(q.params.z, 1.0)) + 1.0;
    vec2 lo = max(q.rect.xy - grow, q.clip.xy);
    vec2 hi = max(min(q.rect.xy + size + grow, q.clip.zw), lo);
    vec2 px = mix(lo, hi, corners[gl_VertexIndex]);
    v_uv = mix(q.uv.xy, q.uv.zw, (px - q.rect.xy) / max(size, vec2(1e-6)));
    v_vcolor = vec4(0.0);
    gl_Position = to_clip(px);
}
