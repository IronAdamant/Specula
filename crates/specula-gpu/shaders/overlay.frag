#version 460
// Overlay quads, fragment stage: rounded-rect coverage (anti-aliased), the texture (colour, or
// red = coverage for font atlases), premultiplied output. Compile with shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#include "overlay.glsl"

layout(set = 0, binding = 1) uniform sampler2D textures[];

layout(location = 0) in vec2 v_local;
layout(location = 1) in vec2 v_uv;
layout(location = 2) flat in vec4 v_color;
layout(location = 3) flat in vec4 v_shape;
layout(location = 4) flat in uvec2 v_info;
layout(location = 5) flat in vec4 v_uv_rect;

layout(location = 0) out vec4 out_color;

void main() {
    vec2 half_size = 0.5 * v_shape.xy;
    float r = clamp(v_shape.z, 0.0, min(half_size.x, half_size.y));
    vec2 p = abs(v_local - half_size) - (half_size - r);
    float sd = length(max(p, vec2(0.0))) + min(max(p.x, p.y), 0.0) - r;
    float cover = clamp(0.5 - sd, 0.0, 1.0);
    vec4 c = v_color;
    if (v_info.x != 0u) {
        // Clamp to the quad's own uv rect so atlas neighbours never bleed in.
        vec2 uv = clamp(v_uv, min(v_uv_rect.xy, v_uv_rect.zw), max(v_uv_rect.xy, v_uv_rect.zw));
        vec4 t = texture(textures[nonuniformEXT(v_info.x - 1u)], uv);
        if (v_info.y == 1u) {
            c.a *= t.r;
        } else {
            c *= t;
        }
    }
    float a = clamp(c.a, 0.0, 1.0) * cover;
    out_color = vec4(c.rgb * a, a);
}
