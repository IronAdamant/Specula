// Specula ID buffer, opaque draws: the draw's id into the R32_UINT target (depth tested and
// written, so each pixel ends with the nearest opaque draw). TEXTURED: textured draws, whose
// alpha-masked texels the scene cuts (mesh_frag.glsl) write nothing, as there. Included by
// ids.frag and ids_tex.frag. See src/idbuffer.rs. Compile with shaders/build.sh.
#include "ids.glsl"

layout(location = 0) flat in uvec2 v_ids; // x: the id, y: the record index
layout(location = 1) in vec3 v_world;
layout(location = 2) in vec3 v_normal;
#ifdef TEXTURED
layout(location = 3) in vec2 v_uv;
#endif
layout(location = 0) out uint out_id;

void main() {
#ifdef TEXTURED
    if ((pc.faces & FACES_FOLIAGE) != 0u && foliage_faded(pc.frame, pc.draws.items[v_ids.y].ext.w, v_world, gl_FragCoord.xy)) {
        discard; // faded foliage, as mesh_frag.glsl cuts it (textured draws only)
    }
    if (cut_out(v_ids.y, v_uv, v_normal, v_world)) {
        discard;
    }
#endif
    out_id = v_ids.x;
}
