#version 460
// Particles, fragment stage (src/effects.rs): the texel times the particle's colour, tested
// against the scene's depth here (the pass has no depth attachment: it reads the depth as a
// texture), faded over `soft` metres where it meets geometry, blended premultiplied with alpha
// scaled by 1 - additive (additive particles add, alpha ones cover; one pipeline, one draw).
// Compile with shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#include "particle.glsl"
#include "effects_light.glsl"

layout(set = 0, binding = 1) uniform sampler2D textures[];

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec3 v_world;
layout(location = 2) flat in vec4 v_color;
layout(location = 3) flat in uint v_slot;
layout(location = 4) flat in vec2 v_params;

layout(location = 0) out vec4 out_color;

// The scene's depth under this pixel as distance along the view axis (from the eye, or from the
// near plane of an orthographic view); far away for the sky.
float scene_distance(ivec2 px) {
    float d = texelFetch(textures[pc.depth], px, 0).r;
    bool reversed = (pc.mode & 1u) != 0u;
    if (reversed ? d <= 0.0 : d >= 1.0) {
        return 1e30;
    }
    vec2 ndc = (vec2(px) + 0.5) / pc.frame.viewport.xy * 2.0 - 1.0;
    vec4 w = pc.frame.inv_view_proj * vec4(ndc, d, 1.0);
    return dot(w.xyz / w.w - pc.cam_pos.xyz, pc.cam_forward.xyz);
}

void main() {
    // Sampled first, while every pixel of the quad is still running (implicit derivatives).
    vec4 tex = v_slot != 0u ? texture(textures[nonuniformEXT(v_slot - 1u)], v_uv) : vec4(1.0);
    float mine = dot(v_world - pc.cam_pos.xyz, pc.cam_forward.xyz);
    float scene = scene_distance(ivec2(gl_FragCoord.xy));
    if (scene < mine) {
        discard;
    }
    float fade = v_params.x > 0.0 ? clamp((scene - mine) / v_params.x, 0.0, 1.0) : 1.0;
    float a = clamp(tex.a * v_color.a * fade, 0.0, 1.0);
    if (a < 1.0 / 512.0) {
        discard;
    }
    vec3 c = display(v_color.rgb * tex.rgb);
    out_color = vec4(c * a, a * (1.0 - v_params.y));
}
