#version 460
// Depth only (no fragment stage): opaque draws through the camera, into the SSAO depth image.
// Compile with shaders/build.sh.
#include "frame.glsl"

layout(location = 0) in vec3 in_position;

// Must match `PushConstants` in src/render.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;   // instance i reads items[gl_InstanceIndex]
    uint material;
    uint pass;
} pc;

void main() {
    gl_Position = pc.frame.view_proj * (pc.draws.items[gl_InstanceIndex].model * vec4(in_position, 1.0));
}
