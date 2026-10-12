#version 450
// Specula M0 mesh pipeline, vertex stage.
// Recompile by hand (no build scripts):
//   glslc -O --target-env=vulkan1.3 mesh.vert -o mesh.vert.spv
//   spirv-val --target-env vulkan1.3 mesh.vert.spv

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;

// 128 bytes, shared with mesh.frag; must match `PushConstants` in src/render.rs.
layout(push_constant) uniform Push {
    mat4 mvp;              // projection * view * model, column-major
    vec4 normal_matrix[3]; // columns of inverse-transpose(model 3x3), w unused
    vec4 color;            // linear RGB, a unused
} pc;

layout(location = 0) out vec3 v_normal; // world space, not normalized

void main() {
    gl_Position = pc.mvp * vec4(in_position, 1.0);
    mat3 nm = mat3(pc.normal_matrix[0].xyz, pc.normal_matrix[1].xyz, pc.normal_matrix[2].xyz);
    v_normal = nm * in_normal;
}
