#version 460
// Overlay quads, vertex stage: instance i is quad i, six vertices each. The quad grows by a pixel
// on every side so the fragment stage can anti-alias fractional and rounded edges.
// Compile with shaders/build.sh.
#include "overlay.glsl"

layout(location = 0) out vec2 v_local;       // pixels from the rect's top-left corner
layout(location = 1) out vec2 v_uv;
layout(location = 2) flat out vec4 v_color;
layout(location = 3) flat out vec4 v_shape;  // width, height, radius
layout(location = 4) flat out uvec2 v_info;
layout(location = 5) flat out vec4 v_uv_rect;

void main() {
    const vec2 corners[6] = vec2[](vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0));
    Quad q = pc.quads.items[gl_InstanceIndex];
    vec2 size = max(q.rect.zw, vec2(0.0));
    vec2 local = corners[gl_VertexIndex] * (size + 2.0) - 1.0;
    v_local = local;
    v_uv = mix(q.uv.xy, q.uv.zw, local / max(size, vec2(1e-6)));
    v_color = q.color;
    v_shape = vec4(size, q.params.x, 0.0);
    v_info = q.info.xy;
    v_uv_rect = q.uv;
    vec2 px = q.rect.xy + local;
    gl_Position = vec4(px / pc.frame.viewport.xy * 2.0 - 1.0, 0.0, 1.0);
}
