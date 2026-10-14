#version 460
// Specula ID buffer, vertex stage: each draw's position only, plus the caller's id for its record.
// Sight's visible-pixel counts and exact screen extents come from this pass (see src/idbuffer.rs).
// Compile with shaders/build.sh.
#include "frame.glsl"

layout(location = 0) in vec3 in_position;

// One id per draw record, in record order; written by `IdUpload` in src/idbuffer.rs.
layout(buffer_reference, std430, buffer_reference_align = 4) readonly buffer Ids {
    uint items[];
};

// Must match `IdPush` in src/idbuffer.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;   // instance i reads items[gl_InstanceIndex]
    Ids ids;       // and ids.items[gl_InstanceIndex]
} pc;

layout(location = 0) flat out uint v_id;

void main() {
    vec4 world = pc.draws.items[gl_InstanceIndex].model * vec4(in_position, 1.0);
    gl_Position = pc.frame.view_proj * world;
    v_id = pc.ids.items[gl_InstanceIndex];
}
