// Specula mesh pipeline, vertex stage (shared by every material), included by the entry files:
//   mesh.vert           Vertex only
//   mesh_skin.vert      + SKINNED: joints/weights (binding 1), bone matrices from `pc.bones`
//   mesh_tex.vert       + TEXTURED: UVs and tangents (binding 2) for textured materials
//   mesh_skin_tex.vert  both
// Compile with shaders/build.sh.

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec4 in_color; // sRGB-encoded RGBA (unorm8)
#ifdef SKINNED
layout(location = 3) in uvec4 in_joints;  // bone indices into the draw's palette (uint8)
layout(location = 4) in vec4 in_weights;  // unorm8, summing to 1
#endif
#ifdef TEXTURED
layout(location = 5) in vec2 in_uv;
layout(location = 6) in vec4 in_tangent;  // xyz object-space tangent, w bitangent sign (snorm16)
#endif

#include "frame.glsl"

// Set once per instanced batch; must match `PushConstants` in src/render.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;   // instance i reads items[gl_InstanceIndex] (firstInstance = the batch's first)
    uint material;
    uint pass;
    Bones bones;   // skinned draws: the palette starts at items[i].ext.z
} pc;

layout(location = 0) out vec3 v_world;  // world position
layout(location = 1) out vec3 v_normal; // world normal, not normalized
layout(location = 2) out vec3 v_local;  // object-space position
layout(location = 3) flat out uint v_draw; // record index
layout(location = 4) out vec4 v_tint;      // linear vertex colour, a = alpha
#ifdef TEXTURED
layout(location = 5) out vec2 v_uv;
layout(location = 6) out vec4 v_tangent;   // world tangent, w bitangent sign
#endif

void main() {
    mat4 model = pc.draws.items[gl_InstanceIndex].model;
#ifdef SKINNED
    // Bind pose -> pose, then object -> world.
    model = model * skin_matrix(pc.bones, pc.draws.items[gl_InstanceIndex].ext.z, in_joints, in_weights);
#endif
    vec4 world = model * vec4(in_position, 1.0);
    gl_Position = pc.frame.view_proj * world;
    // Inverse-transpose of the model's 3x3: cofactor columns (b x c, c x a, a x b) / det.
    mat3 m = mat3(model);
    mat3 cof = mat3(cross(m[1], m[2]), cross(m[2], m[0]), cross(m[0], m[1]));
    float det = dot(m[0], cof[0]);
    v_normal = cof * in_normal / (abs(det) > 1e-20 ? det : 1.0);
    v_world = world.xyz;
    v_local = in_position;
    v_draw = uint(gl_InstanceIndex);
    // sRGB -> linear; white stays exactly 1 so an untinted draw is unchanged bit for bit.
    vec3 c = in_color.rgb;
    vec3 lin = mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), greaterThan(c, vec3(0.04045)));
    v_tint = vec4(mix(lin, vec3(1.0), greaterThanEqual(c, vec3(1.0))), in_color.a);
#ifdef TEXTURED
    v_uv = in_uv;
    // Tangents move with the surface; a mirroring transform flips the bitangent.
    v_tangent = vec4(m * in_tangent.xyz, det < 0.0 ? -in_tangent.w : in_tangent.w);
#endif
}
