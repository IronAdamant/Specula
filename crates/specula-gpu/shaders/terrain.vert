#version 460
// Terrain patches, vertex stage (src/terrain.rs): one instance per selected CDLOD node, the
// vertex from the terrain's heights, morphed to the next level with distance (terrain.glsl).
// Compile with shaders/build.sh.
#include "frame.glsl"
#include "terrain.glsl"

layout(location = 0) in vec3 in_position; // grid coordinates (x, ring, z)

// Must match `PushConstants` in src/render.rs (the fifth field: the terrain records).
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;
    uint material;
    uint pass;
    Terrains terrains;
} pc;

layout(location = 0) out vec3 v_world;
layout(location = 1) out vec2 v_cell;         // sample coordinates
layout(location = 2) flat out uint v_terrain; // the terrain record's index
// The model's inverse transpose up to scale (cofactor columns): local normals into the world.
layout(location = 3) flat out vec3 v_cof0;
layout(location = 4) flat out vec3 v_cof1;
layout(location = 5) flat out vec3 v_cof2;

void main() {
    Draw d = pc.draws.items[gl_InstanceIndex];
    TerrainVertex v = terrain_vertex(pc.frame, d, pc.terrains, in_position);
    gl_Position = pc.frame.view_proj * vec4(v.world, 1.0);
    v_world = v.world;
    v_cell = v.cell;
    v_terrain = d.ext.x;
    mat3 m = mat3(d.model);
    v_cof0 = cross(m[1], m[2]);
    v_cof1 = cross(m[2], m[0]);
    v_cof2 = cross(m[0], m[1]);
}
