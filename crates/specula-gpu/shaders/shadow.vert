#version 460
// Specula shadow pipeline, vertex stage (depth only, no fragment stage): shadow casters into
// cascade `pc.pass` of the sun's shadow map.
// Compile with shaders/build.sh.
#include "frame.glsl"

layout(location = 0) in vec3 in_position;

// Must match `PushConstants` in src/render.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;   // instance i reads items[gl_InstanceIndex]
    uint material;
    uint pass;     // cascade
} pc;

void main() {
    vec4 world = pc.draws.items[gl_InstanceIndex].model * vec4(in_position, 1.0);
    gl_Position = pc.frame.shadow_mats[pc.pass] * world;
}
