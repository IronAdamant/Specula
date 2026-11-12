#version 460
// Entry point: unmasked swaying shadow casters (see shadow_vert.glsl), cut near the wind's
// shadow_end by shadow_sway.frag. Compile with shaders/build.sh.
#define SWAY
#include "shadow_vert.glsl"
