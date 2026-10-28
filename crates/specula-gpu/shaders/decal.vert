#version 460
// Decals, vertex stage (src/effects.rs): instance i is decal i, 36 vertices its box. The pipeline
// culls front faces, so each pixel the box covers is shaded once by a back face, with the camera
// outside the box or inside it. The record goes to the fragment stage as flat varyings.
// Compile with shaders/build.sh.
#include "decal.glsl"

layout(location = 0) flat out vec4 v_inv0;
layout(location = 1) flat out vec4 v_inv1;
layout(location = 2) flat out vec4 v_inv2;
layout(location = 3) flat out vec4 v_inv3;
layout(location = 4) flat out vec4 v_up;
layout(location = 5) flat out vec4 v_color;
layout(location = 6) flat out vec4 v_uv;
layout(location = 7) flat out vec4 v_params;
layout(location = 8) flat out uvec4 v_info;
layout(location = 9) flat out vec3 v_axis_x;

// The cube's 12 triangles (corner bits: x = 1, y = 2, z = 4), wound outward counter-clockwise.
const uint CUBE[36] = uint[](
    0u, 2u, 3u, 0u, 3u, 1u,   // -z
    4u, 5u, 7u, 4u, 7u, 6u,   // +z
    0u, 4u, 6u, 0u, 6u, 2u,   // -x
    1u, 3u, 7u, 1u, 7u, 5u,   // +x
    0u, 1u, 5u, 0u, 5u, 4u,   // -y
    2u, 6u, 7u, 2u, 7u, 3u);  // +y

void main() {
    DecalRec d = pc.decals.items[gl_InstanceIndex];
    uint c = CUBE[gl_VertexIndex];
    vec3 corner = vec3(float(c & 1u), float((c >> 1u) & 1u), float((c >> 2u) & 1u)) - 0.5;
    gl_Position = pc.frame.view_proj * (d.model * vec4(corner, 1.0));
    v_inv0 = d.inv[0];
    v_inv1 = d.inv[1];
    v_inv2 = d.inv[2];
    v_inv3 = d.inv[3];
    v_up = d.up;
    v_color = d.color;
    v_uv = d.uv;
    v_params = d.params;
    v_info = d.info;
    v_axis_x = normalize(d.model[0].xyz);
}
