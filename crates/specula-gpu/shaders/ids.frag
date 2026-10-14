#version 460
// Specula ID buffer, fragment stage: the draw's id into both channels of an R32G32_UINT target.
// The opaque pipeline writes only R (depth tested and written), the blended one only G (depth
// tested, not written): see src/idbuffer.rs.
// Compile with shaders/build.sh.

layout(location = 0) flat in uint v_id;
layout(location = 0) out uvec2 out_id;

void main() {
    out_id = uvec2(v_id, v_id);
}
