// Specula ID buffer, opaque draws: the draw's id into the R32_UINT target (depth tested and
// written, so each pixel ends with the nearest opaque draw). TEXTURED: textured draws, whose
// alpha-masked texels the scene cuts (mesh_frag.glsl) write nothing, as there. Included by
// ids.frag and ids_tex.frag. See src/idbuffer.rs. Compile with shaders/build.sh.
#include "ids.glsl"

layout(location = 0) flat in uvec2 v_ids; // x: the id, y: the record index
#ifdef TEXTURED
layout(location = 3) in vec2 v_uv;
#endif
layout(location = 0) out uint out_id;

void main() {
#ifdef TEXTURED
    if (cut_out(v_ids.y, v_uv)) {
        discard;
    }
#endif
    out_id = v_ids.x;
}
