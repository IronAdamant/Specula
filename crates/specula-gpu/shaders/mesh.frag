#version 450
// Specula M0 mesh pipeline, fragment stage.
// Recompile by hand (no build scripts):
//   glslc -O --target-env=vulkan1.3 mesh.frag -o mesh.frag.spv
//   spirv-val --target-env vulkan1.3 mesh.frag.spv

layout(location = 0) in vec3 v_normal;

layout(push_constant) uniform Push {
    mat4 mvp;
    vec4 normal_matrix[3];
    vec4 color;
} pc;

layout(location = 0) out vec4 out_color;

// World-space direction towards the light: normalize(0.4, 0.8, 0.45).
const vec3 LIGHT_DIR = vec3(0.4, 0.8, 0.45) / length(vec3(0.4, 0.8, 0.45));

void main() {
    vec3 n = normalize(v_normal);
    float lambert = max(dot(n, LIGHT_DIR), 0.0);
    // Linear colour in, sRGB attachment encodes on write.
    out_color = vec4(pc.color.rgb * (0.25 + 0.75 * lambert), 1.0);
}
