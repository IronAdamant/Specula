#version 460
// Terrain patches, vertex stage (src/terrain.rs): one instance per selected CDLOD node, the
// vertex from the terrain's heights, morphed to the next level with distance (terrain.glsl). At GI tier low it samples the lean
// probes once per vertex (shaders/gi.glsl), so the fragment stage adds no GI cost: per pixel it
// cost the iGPU 2.6 ms of a screen-filling terrain at 1600 x 900.
// Compile with shaders/build.sh.
#define GI_FIELDS
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

// The GI code with GI on (this stage has no feature constant: the frame says whether it has GI).
const uint FEATURES = 512u;
#include "gi.glsl"

layout(location = 0) out vec3 v_world;
layout(location = 1) out vec2 v_cell;         // sample coordinates
layout(location = 2) flat out uint v_terrain; // the terrain record's index
// The model's inverse transpose up to scale (cofactor columns): local normals into the world.
layout(location = 3) flat out vec3 v_cof0;
layout(location = 4) flat out vec3 v_cof1;
layout(location = 5) flat out vec3 v_cof2;
// Tier low's ambient, per vertex (the frame's flat ambient otherwise; the fragment stage reads
// it only at tier low).
layout(location = 6) out vec3 v_gi;

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
    // Tier low (a per-draw GI array, gi_params.w): the lean probes for the heights' normal.
    v_gi = pc.frame.ambient.rgb;
    if (pc.frame.gi_params.x > 0.5 && pc.frame.gi_params.w > 0.5) {
        Terrain t = pc.terrains.items[d.ext.x];
        vec2 size = vec2(t.size.xy) - 1.0;
        ivec2 c = ivec2(clamp(v.cell, vec2(0.0), size));
        ivec2 hi = ivec2(size);
        float dx = terrain_height(t.size.z, min(c + ivec2(1, 0), hi)) - terrain_height(t.size.z, max(c - ivec2(1, 0), ivec2(0)));
        float dz = terrain_height(t.size.z, min(c + ivec2(0, 1), hi)) - terrain_height(t.size.z, max(c - ivec2(0, 1), ivec2(0)));
        vec3 nl = vec3(-dx / (2.0 * t.grid.z), 1.0, -dz / (2.0 * t.grid.w));
        vec3 n = normalize(mat3(v_cof0, v_cof1, v_cof2) * nl);
        vec3 view = normalize(pc.frame.camera_time.xyz - v.world);
        v_gi = gi_ambient_lean(v.world, n, view);
    }
}
