// Specula shadow pipeline, vertex stage (depth only, no fragment stage): shadow casters into
// cascade `pc.pass` of the sun's shadow map. Included by shadow.vert and shadow_skin.vert
// (SKINNED). Compile with shaders/build.sh.
#include "frame.glsl"

layout(location = 0) in vec3 in_position;
#ifdef SKINNED
layout(location = 3) in uvec4 in_joints;
layout(location = 4) in vec4 in_weights;
#endif

// Must match `PushConstants` in src/render.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;   // instance i reads items[gl_InstanceIndex]
    uint material;
    uint pass;     // cascade
    Bones bones;
} pc;

void main() {
    mat4 model = pc.draws.items[gl_InstanceIndex].model;
#ifdef SKINNED
    model = model * skin_matrix(pc.bones, pc.draws.items[gl_InstanceIndex].ext.z, in_joints, in_weights);
#endif
    vec4 world = model * vec4(in_position, 1.0);
    gl_Position = pc.frame.shadow_mats[pc.pass] * world;
}
