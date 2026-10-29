#version 460
// Sight's ID buffer, terrain patches (src/idbuffer.rs): the scene's patch vertices
// (terrain.glsl, the same morph) with the caller's id for the record. Drawn by the opaque step
// (ids.frag) and the raster step (ids_count.frag). Compile with shaders/build.sh.
#extension GL_EXT_buffer_reference_uvec2 : require
#include "ids.glsl"
#include "terrain.glsl"

layout(location = 0) in vec3 in_position;

layout(location = 0) flat out uvec2 v_ids; // x: the id, y: the record index
layout(location = 1) out vec3 v_world;
layout(location = 2) out vec3 v_normal;

void main() {
    // For terrain batches the push's bones address holds the terrain records.
    Terrains ts = Terrains(uvec2(pc.bones));
    TerrainVertex v = terrain_vertex(pc.frame, pc.draws.items[gl_InstanceIndex], ts, in_position);
    gl_Position = pc.frame.view_proj * vec4(v.world, 1.0);
    v_ids = uvec2(pc.ids.items[gl_InstanceIndex], uint(gl_InstanceIndex));
    v_world = v.world;
    v_normal = vec3(0.0, 1.0, 0.0);
}
