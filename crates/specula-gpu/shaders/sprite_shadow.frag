#version 460
// Sprite shadow casters, fragment stage: the alpha test of the cutout sprite (depth only).
// Compile with shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#include "sprite.glsl"

layout(set = 0, binding = 1) uniform sampler2D textures[];

layout(location = 0) in vec2 v_uv;
layout(location = 1) flat in uvec4 v_info;
layout(location = 2) flat in vec4 v_rect;
layout(location = 3) flat in float v_cutoff;
layout(location = 4) flat in float v_alpha;

void main() {
    float a = v_alpha;
    if (v_info.x != 0u) {
        uint slot = v_info.x - 1u;
        vec2 size = vec2(textureSize(textures[nonuniformEXT(slot)], 0));
        vec2 lo = min(v_rect.xy, v_rect.zw) * size;
        vec2 hi = max(v_rect.xy, v_rect.zw) * size;
        vec2 t = v_uv * size;
        if ((v_info.y & FLAG_NEAREST) != 0u) {
            vec2 first = floor(lo + 1e-3);
            t = clamp(floor(t), first, max(ceil(hi - 1e-3) - 1.0, first)) + 0.5;
        } else {
            t = clamp(t, lo + 0.5, max(hi - 0.5, lo + 0.5));
        }
        a *= textureLod(textures[nonuniformEXT(slot)], t / size, 0.0).a;
    }
    if (a < v_cutoff) {
        discard;
    }
}
