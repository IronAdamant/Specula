// Specula ID buffer, vertex stage (every ID pipeline that draws meshes), included by the entry
// files, which mirror the scene's (mesh_vert.glsl):
//   ids.vert           Vertex position and normal only
//   ids_skin.vert      + SKINNED: joints/weights (binding 1), bone matrices from `pc.bones`
//   ids_tex.vert       + TEXTURED: UVs (binding 2), for the alpha-mask cut
//   ids_skin_tex.vert  both
// Each draw's position, its world normal (the lava lamp's face rule), and the caller's id for its
// record. See src/idbuffer.rs. Compile with shaders/build.sh.
#include "ids.glsl"

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
#ifdef SKINNED
layout(location = 3) in uvec4 in_joints;  // bone indices into the draw's palette (uint8)
layout(location = 4) in vec4 in_weights;  // unorm8, summing to 1
#endif
#ifdef TEXTURED
layout(location = 5) in vec2 in_uv;
#endif

layout(location = 0) flat out uvec2 v_ids; // x: the id, y: the record index
layout(location = 1) out vec3 v_world;
layout(location = 2) out vec3 v_normal; // not normalized
#ifdef TEXTURED
layout(location = 3) out vec2 v_uv;
#endif

// The vertex's object -> world matrix. THE extension point: anything that moves vertices in
// mesh_vert.glsl (skinning, morphs, wind) must move them here identically, or the ID buffer
// disagrees with the image.
mat4 id_model() {
    mat4 model = pc.draws.items[gl_InstanceIndex].model;
#ifdef SKINNED
    // Bind pose -> pose, then object -> world (as mesh_vert.glsl).
    model = model * skin_matrix(pc.bones, pc.draws.items[gl_InstanceIndex].ext.z, in_joints, in_weights);
#endif
    return model;
}

void main() {
    mat4 model = id_model();
    vec4 world = model * vec4(in_position, 1.0);
    gl_Position = pc.frame.view_proj * world;
    v_ids = uvec2(pc.ids.items[gl_InstanceIndex], uint(gl_InstanceIndex));
    v_world = world.xyz;
    // Inverse-transpose of the model's 3x3 via cofactors, as mesh.vert (only the sign matters).
    mat3 m = mat3(model);
    mat3 cof = mat3(cross(m[1], m[2]), cross(m[2], m[0]), cross(m[0], m[1]));
    float det = dot(m[0], cof[0]);
    v_normal = cof * in_normal * (det < 0.0 ? -1.0 : 1.0);
#ifdef TEXTURED
    v_uv = in_uv;
#endif
}
