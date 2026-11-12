#version 460
// Specula shadow pipeline, fragment stage of unmasked swaying casters: no colour, only the fade
// near the wind's shadow_end (frame.glsl sway_shadow_faded); everything else casts solid, as in
// shadow.vert's depth-only pipeline. Compile with shaders/build.sh.
#include "frame.glsl"

layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;
    uint material;
    uint pass;
    Bones bones;
} pc;

layout(location = 0) in vec3 v_world;
layout(location = 1) flat in uint v_draw;

void main() {
    if (sway_shadow_faded(pc.frame, pc.draws.items[v_draw].ext.w, v_world, gl_FragCoord.xy)) {
        discard;
    }
}
