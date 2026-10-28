// Shared by decal.vert and decal.frag (src/effects.rs). Included; not compiled on its own.
#include "frame.glsl"

// One decal; must match `DecalRecord` in src/effects.rs (std430, 208 bytes).
//   model: unit box (-0.5..0.5) -> world      inv: world -> the box
//   up: the box's +Y in the world (unit), w = angle fade (cosine where it starts)
//   color: linear tint, a = opacity            uv: texture rect u0 v0 u1 v1
//   params: roughness, specular, emissive, normal strength
//   info: albedo slot + 1, normal slot + 1, unused x2
struct DecalRec {
    mat4 model;
    mat4 inv;
    vec4 up;
    vec4 color;
    vec4 uv;
    vec4 params;
    uvec4 info;
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Decals {
    DecalRec items[];
};

// Must match `DecalPush` in src/effects.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Decals decals;
    uint depth; // bindless slot of the scene's depth
    uint mode;  // bit 0: reversed Z
    vec4 cam_pos;
} pc;

layout(constant_id = 0) const uint FEATURES = 0u;
const bool SHADOWS = (FEATURES & 1u) != 0u;
const bool FOG = (FEATURES & 2u) != 0u;
const bool HDR = (FEATURES & 4u) != 0u;
const bool SSAO = (FEATURES & 32u) != 0u;
const bool MULTIPLY = (FEATURES & 256u) != 0u;
