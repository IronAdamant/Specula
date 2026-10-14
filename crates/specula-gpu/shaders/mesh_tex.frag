#version 460
// Entry point: the Lit path with material textures (see mesh_frag.glsl). Compile with
// shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#define TEXTURED
#include "mesh_frag.glsl"
