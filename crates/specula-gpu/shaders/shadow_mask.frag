#version 460
// Specula shadow pipeline, fragment stage of alpha-masked casters: no colour, only the cut —
// texels of the base colour whose alpha is below the draw's cutoff cast no shadow (as the scene
// pass cuts them). Compile with shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#include "frame.glsl"

layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;
    uint material;
    uint pass;
    Bones bones;
} pc;

layout(set = 0, binding = 1) uniform sampler2D textures[];

layout(location = 0) in vec2 v_uv;
layout(location = 1) flat in uint v_draw;

void main() {
    uvec4 ext = pc.draws.items[v_draw].ext;
    uint base_slot = ext.x & 0xffffu;
    if (base_slot != 0u && (ext.w & EXT_ALPHA_MASK) != 0u) {
        float a = texture(textures[nonuniformEXT(base_slot - 1u)], v_uv).a;
        if (a < float((ext.w >> 8) & 255u) / 255.0) {
            discard;
        }
    }
}
