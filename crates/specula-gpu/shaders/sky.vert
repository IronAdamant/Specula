#version 460
// Specula sky, vertex stage: a full-screen triangle on the far plane (depth 0 with reversed Z,
// 1 otherwise), so with the EQUAL depth test it only covers pixels no geometry was drawn on.
// Compile with shaders/build.sh.
#include "frame.glsl"

// Must match `PostPush` in src/post.rs (mode = 1 for reversed Z).
layout(push_constant) uniform Push {
    Frame frame;
    uint src;
    uint src2;
    vec2 texel;
    uint mode;
    uint pad;
    vec4 params;
} pc;

layout(location = 0) out vec2 v_uv;
// The view ray's points on the near plane and halfway into the depth range, homogeneous: linear
// in screen position, so interpolated exactly and divided per pixel (no per-pixel matrices).
layout(location = 1) out vec4 v_near;
layout(location = 2) out vec4 v_mid;

void main() {
    vec2 p = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
    v_uv = p;
    vec2 ndc = p * 2.0 - 1.0;
    v_near = pc.frame.inv_view_proj * vec4(ndc, pc.mode == 1u ? 1.0 : 0.0, 1.0);
    v_mid = pc.frame.inv_view_proj * vec4(ndc, 0.5, 1.0);
    gl_Position = vec4(ndc, pc.mode == 1u ? 0.0 : 1.0, 1.0);
}
