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
layout(location = 2) in vec3 v_world;
layout(location = 3) in vec3 v_normal;

void main() {
    uvec4 ext = pc.draws.items[v_draw].ext;
    // Faded foliage casts no shadow either (the dither in the shadow map's pixels).
    if (foliage_faded(pc.frame, ext.w, v_world, gl_FragCoord.xy)) {
        discard;
    }
    // Near the wind's shadow_end the caster thins out (frame.glsl sway_shadow_faded).
    if (sway_shadow_faded(pc.frame, ext.w, v_world, gl_FragCoord.xy)) {
        discard;
    }
    uint base_slot = ext.x & 0xffffu;
    if (base_slot != 0u && (ext.w & EXT_ALPHA_MASK) != 0u) {
        // Foliage cards edge-on to the sun cast nothing: their blades would draw dashed lines on
        // the ground (frame.glsl foliage_edge, from the sun's direction instead of the camera's).
        float a = texture(textures[nonuniformEXT(base_slot - 1u)], v_uv).a * foliage_edge(ext.w, v_normal, v_world, v_world + pc.frame.sun_dir.xyz);
        if (a < float((ext.w >> 8) & 255u) / 255.0) {
            discard;
        }
    }
}
