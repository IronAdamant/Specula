#version 460
// Terrain patches into cascade `pc.pass` of the sun's shadow map: the same vertices the scene
// draws (terrain.glsl, morphed by the camera's distance, not the light's). Compile with
// shaders/build.sh.
#include "frame.glsl"
#include "terrain.glsl"

layout(location = 0) in vec3 in_position;

// Must match `PushConstants` in src/render.rs (the fifth field: the terrain records).
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;
    uint material;
    uint pass; // cascade
    Terrains terrains;
} pc;

void main() {
    TerrainVertex v = terrain_vertex(pc.frame, pc.draws.items[gl_InstanceIndex], pc.terrains, in_position);
    gl_Position = pc.frame.shadow_mats[pc.pass] * vec4(v.world, 1.0);
}
