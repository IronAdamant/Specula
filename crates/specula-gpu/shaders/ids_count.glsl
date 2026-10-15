// Specula ID buffer, blended draws (no colour output; needs fragmentStoresAndAtomics).
// MODE 0, insert: depth tested against the opaque depth (never written); the fragment's id joins
//   its pixel's set of the SEEN_SLOTS smallest ids. The final sets don't depend on the
//   order fragments run in; ids_resolve.frag counts them afterwards (one count per id per pixel).
//   An id pushed out of a set (more than SEEN_SLOTS ids at the pixel) is marked incomplete: its
//   resolved count and box are then lower bounds. Equally reproducible, and nothing is counted
//   here (no contended atomics, whatever the number of layers).
// MODE 1, raster: every draw, no depth test: marks each id rasterized at all.
// Included by ids_count.frag and ids_count_tex.frag (TEXTURED: textured draws, whose alpha-masked
// texels the scene cuts). See src/idbuffer.rs. Compile with shaders/build.sh.
#include "ids.glsl"

// Side effects only for fragments that pass the depth test.
layout(early_fragment_tests) in;

layout(constant_id = 0) const uint MODE = 0u;

layout(location = 0) flat in uvec2 v_ids; // x: the id, y: the record index
layout(location = 1) in vec3 v_world;
layout(location = 2) in vec3 v_normal;
#ifdef TEXTURED
layout(location = 3) in vec2 v_uv;
#endif

// Whether the scene shows this fragment: mesh.frag's discards, mirrored (every mode).
bool shown() {
#ifdef TEXTURED
    if (cut_out(v_ids.y, v_uv)) {
        return false; // an alpha-masked texel (mesh_frag.glsl discards it)
    }
#endif
    if (pc.faces == FACES_LAVA) {
        // shade_lava_lamp: shaded on the proxy's near faces, or on its far faces when the camera
        // is inside the box.
        vec3 cam = pc.frame.camera_time.xyz;
        bool near_face = dot(v_normal, cam - v_world) > 0.0;
        mat4 m = pc.draws.items[v_ids.y].model;
        vec3 scale = vec3(length(m[0].xyz), length(m[1].xyz), length(m[2].xyz));
        mat3 rot = mat3(m[0].xyz / scale.x, m[1].xyz / scale.y, m[2].xyz / scale.z);
        vec3 ro = transpose(rot) * (cam - m[3].xyz);
        bool inside_box = all(lessThan(abs(ro), 0.5 * scale));
        return near_face != inside_box;
    }
    return true;
}

void main() {
    uint id = v_ids.x;
    if (id == 0u || !shown()) {
        return;
    }
    if (MODE == 1u) {
        id_rasterized(id_target(), id);
    } else {
        id_insert(id_target(), id, uvec2(gl_FragCoord.xy));
    }
}
