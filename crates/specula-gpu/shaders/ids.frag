#version 460
// Specula ID buffer, opaque draws: the draw's id into the R32_UINT target (depth tested and
// written, so each pixel ends with the nearest opaque draw). See src/idbuffer.rs.
// Compile with shaders/build.sh.

layout(location = 0) flat in uvec2 v_ids; // x: the id
layout(location = 0) out uint out_id;

void main() {
    out_id = v_ids.x;
}
