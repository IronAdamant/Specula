// Shared by particle.vert and particle.frag (src/effects.rs). Included; not compiled on its own.
#include "frame.glsl"

// One particle; must match `ParticleRecord` in src/effects.rs (std430, 48 bytes).
//   a = position.xyz, size (m)
//   b = colour rg (half floats), colour ba (half floats), material | frame << 16, rotation bits
//   c = stretch vector.xyz (world), additive (0 = alpha, 1 = additive)
struct ParticleRec {
    vec4 a;
    uvec4 b;
    vec4 c;
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Particles {
    ParticleRec items[];
};

// One material; must match `MaterialRecord` (32 bytes).
//   info = texture slot + 1 (0 = none), flipbook columns, rows, flags (1 = lit)
//   params = soft fade distance (m), unused x3
struct MaterialRec {
    uvec4 info;
    vec4 params;
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Materials {
    MaterialRec items[];
};

// Must match `ParticlePush` in src/effects.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Particles particles;
    Materials materials;
    uint depth; // bindless slot of the scene's depth
    uint mode;  // bit 0: reversed Z, bit 1: orthographic
    vec4 cam_pos;     // perspective: the eye; orthographic: the centre of the near plane
    vec4 cam_right;
    vec4 cam_up;
    vec4 cam_forward;
} pc;

layout(constant_id = 0) const uint FEATURES = 0u;
const bool FOG = (FEATURES & 2u) != 0u;
const bool HDR = (FEATURES & 4u) != 0u;

const uint MAT_LIT = 1u;

bool ortho_view() { return (pc.mode & 2u) != 0u; }
