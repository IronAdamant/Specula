// Specula shadow pipeline, vertex stage: shadow casters into cascade `pc.pass` of the sun's
// shadow map. Included by shadow.vert, shadow_sway.vert (SWAY: unmasked swaying casters pass
// their world position to shadow_sway.frag), shadow_skin.vert (SKINNED), and shadow_mask.vert /
// shadow_skin_mask.vert (MASKED: alpha-masked materials pass their UVs to shadow_mask.frag,
// which cuts the shadow where the base colour's alpha is). Compile with shaders/build.sh.
#include "frame.glsl"

layout(location = 0) in vec3 in_position;
#ifndef SKINNED
layout(location = 2) in vec4 in_color; // the wind's painted weight (alpha)
#endif
#ifdef SKINNED
layout(location = 3) in uvec4 in_joints;
layout(location = 4) in vec4 in_weights;
#endif
#ifdef SWAY
layout(location = 0) out vec3 v_world; // shadow_sway.frag: the fade near the wind's shadow_end
layout(location = 1) flat out uint v_draw;
#endif
#ifdef MASKED
layout(location = 1) in vec3 in_normal;
layout(location = 5) in vec2 in_uv;
layout(location = 0) out vec2 v_uv;
layout(location = 1) flat out uint v_draw;
layout(location = 2) out vec3 v_world; // the foliage fade (shadow_mask.frag)
layout(location = 3) out vec3 v_normal; // foliage cards edge-on to the sun (shadow_mask.frag)
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
#ifndef SKINNED
    // The wind, exactly as mesh_vert.glsl sways the draw.
    world.xyz += wind_offset(pc.frame, pc.draws.items[gl_InstanceIndex].ext.w, in_position, world.xyz, in_color.a, wind_scale(model));
#endif
    gl_Position = pc.frame.shadow_mats[pc.pass] * world;
#ifdef SWAY
    v_world = world.xyz;
    v_draw = uint(gl_InstanceIndex);
#endif
#ifdef MASKED
    v_uv = in_uv;
    v_draw = uint(gl_InstanceIndex);
    v_world = world.xyz;
    v_normal = mat3(model) * in_normal; // a direction is enough: only its angle to the sun is used
#endif
}
