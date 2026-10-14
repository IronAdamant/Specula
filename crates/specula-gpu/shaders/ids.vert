#version 460
// Specula ID buffer, vertex stage (every ID pipeline): each draw's position, its world normal
// (blended counting picks faces by normal, like glass in mesh.frag), and the caller's id for its
// record. See src/idbuffer.rs. Compile with shaders/build.sh.
#include "ids.glsl"

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;

layout(location = 0) flat out uint v_id;
layout(location = 1) out vec3 v_world;
layout(location = 2) out vec3 v_normal; // not normalized

// The vertex in world space. THE extension point: anything that moves vertices in mesh.vert
// (GPU skinning, morphs, wind) must move them here identically, or the ID buffer disagrees with
// the image. A skinned variant replaces this function (same inputs as mesh.vert's skinning), and
// `id_model()` below with the skinned matrix for normals.
mat4 id_model() {
    return pc.draws.items[gl_InstanceIndex].model;
}

vec4 id_world_position() {
    return id_model() * vec4(in_position, 1.0);
}

void main() {
    vec4 world = id_world_position();
    gl_Position = pc.frame.view_proj * world;
    v_id = pc.ids.items[gl_InstanceIndex];
    v_world = world.xyz;
    // Inverse-transpose of the model's 3x3 via cofactors, as mesh.vert (only the sign matters).
    mat3 m = mat3(id_model());
    mat3 cof = mat3(cross(m[1], m[2]), cross(m[2], m[0]), cross(m[0], m[1]));
    float det = dot(m[0], cof[0]);
    v_normal = cof * in_normal * (det < 0.0 ? -1.0 : 1.0);
}
