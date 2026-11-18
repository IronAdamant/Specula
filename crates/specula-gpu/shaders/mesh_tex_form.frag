#version 460
// Entry point: textured meshes in a form view: lit surfaces as shape alone, their textures put
// aside but for the alpha mask and the normal map (see mesh_frag.glsl, form.glsl). Compile with
// shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#define TEXTURED
#define FORM
#include "mesh_frag.glsl"
