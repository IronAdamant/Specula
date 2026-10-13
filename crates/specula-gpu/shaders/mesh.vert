#version 460
// Specula mesh pipeline, vertex stage (shared by every material).
// Recompile by hand (no build scripts):
//   glslc -O --target-env=vulkan1.3 mesh.vert -o mesh.vert.spv
//   spirv-val --target-env vulkan1.3 mesh.vert.spv
#extension GL_EXT_buffer_reference : require

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec4 in_color; // sRGB-encoded RGBA (unorm8)

// Per-frame data; must match `FrameData` in src/render.rs (and mesh.frag).
struct PointLight {
    vec4 pos_range; // xyz world position, w range (m)
    vec4 color;     // linear RGB x intensity
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Frame {
    mat4 view_proj;
    vec4 camera_time; // xyz camera, w time (s)
    vec4 sun_dir;     // xyz towards the sun
    vec4 sun_color;   // linear RGB x intensity
    vec4 ambient;     // linear RGB x intensity, w exposure
    vec4 sky;         // linear clear colour, w tone mapping on
    vec4 viewport;    // width, height, light count
    PointLight lights[16];
};

// Per-draw records; must match `DrawRecord` in src/render.rs (and mesh.frag).
struct Draw {
    mat4 model;  // object -> world
    vec4 color;  // linear RGB, a opacity
    vec4 color2; // linear RGB, a seed
    vec4 params; // emissive, metallic, roughness, specular
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Draws {
    Draw items[];
};

// Set once per instanced batch; must match `PushConstants` in src/render.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;   // instance i reads items[gl_InstanceIndex] (firstInstance = the batch's first)
    uint material;
    uint pass;
} pc;

layout(location = 0) out vec3 v_world;  // world position
layout(location = 1) out vec3 v_normal; // world normal, not normalized
layout(location = 2) out vec3 v_local;  // object-space position
layout(location = 3) flat out uint v_draw; // record index
layout(location = 4) out vec4 v_tint;      // linear vertex colour, a = alpha

void main() {
    mat4 model = pc.draws.items[gl_InstanceIndex].model;
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
}
