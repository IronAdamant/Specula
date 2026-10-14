// Shared by the post-processing passes (full-screen triangles reading the bindless textures).
// Included by the stage sources; not compiled on its own.
#include "frame.glsl"
#extension GL_EXT_nonuniform_qualifier : require

layout(set = 0, binding = 1) uniform sampler2D textures[];

// Must match `PostPush` in src/post.rs (a pass with its own push block defines CUSTOM_PUSH).
#ifndef CUSTOM_PUSH
layout(push_constant) uniform Push {
    Frame frame;
    uint src;    // texture slot read
    uint src2;   // second input slot (0xffffffff = none)
    vec2 texel;  // 1 / size of `src` in pixels
    uint mode;   // pass-specific
    uint pad;
    vec4 params; // pass-specific
} pc;
#endif

layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 out_color;
