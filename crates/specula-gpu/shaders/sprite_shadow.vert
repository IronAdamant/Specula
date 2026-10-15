#version 460
// Sprite shadow casters, vertex stage: the sprite's quad as the camera sees it (so a billboard's
// shadow matches the card that is drawn), into cascade `pc.pass` of the sun's shadow map.
// Compile with shaders/build.sh.
#include "sprite.glsl"

layout(location = 0) out vec2 v_uv;
layout(location = 1) flat out uvec4 v_info;
layout(location = 2) flat out vec4 v_rect;
layout(location = 3) flat out float v_cutoff;
layout(location = 4) flat out float v_alpha;

void main() {
    Item it = pc.items.items[gl_InstanceIndex];
    Corner c = item_corner(it, QUAD[gl_VertexIndex]);
    gl_Position = pc.frame.shadow_mats[pc.pass] * vec4(c.world, 1.0);
    v_uv = c.uv;
    v_info = it.info;
    v_rect = it.v3;
    v_cutoff = it.v5.w;
    v_alpha = it.tint.a;
}
