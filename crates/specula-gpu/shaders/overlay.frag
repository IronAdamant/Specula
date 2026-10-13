#version 460
// Overlay, fragment stage. Quads: rounded-rect coverage with a feathered edge, a vertical fill
// gradient, an inner border with its own gradient, and the texture (colour, or red = coverage
// for font atlases). Triangles: the interpolated vertex colour. Everything is clipped to the
// record's clip rect; output is premultiplied (blended in linear light).
// The same maths as specula-ui's software preview. Compile with shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#include "overlay.glsl"

layout(set = 0, binding = 1) uniform sampler2D textures[];

layout(location = 0) flat in uvec2 v_info;
layout(location = 1) in vec2 v_uv;
layout(location = 2) in vec4 v_vcolor;
layout(location = 3) flat in vec4 v_rect;
layout(location = 4) flat in vec4 v_color;
layout(location = 5) flat in vec4 v_color2;
layout(location = 6) flat in vec4 v_border0;
layout(location = 7) flat in vec4 v_border1;
layout(location = 8) flat in vec4 v_params;
layout(location = 9) flat in vec4 v_clip;
layout(location = 10) flat in vec4 v_uv_rect;

layout(location = 0) out vec4 out_color;

void main() {
    vec2 frag = gl_FragCoord.xy;
    if (any(lessThan(frag, v_clip.xy)) || any(greaterThanEqual(frag, v_clip.zw))) {
        discard;
    }
    if (v_info.y == KIND_TRIANGLE) {
        float a = clamp(v_vcolor.a, 0.0, 1.0);
        out_color = vec4(v_vcolor.rgb * a, a);
        return;
    }
    vec2 size = max(v_rect.zw, vec2(0.0));
    vec2 local = frag - v_rect.xy;
    vec2 half_size = 0.5 * size;
    float r = clamp(v_params.x, 0.0, min(half_size.x, half_size.y));
    vec2 p = abs(local - half_size) - (half_size - r);
    float sd = length(max(p, vec2(0.0))) + min(max(p.x, p.y), 0.0) - r;
    float outer = clamp(0.5 - sd / max(v_params.z, 1.0), 0.0, 1.0);
    float t = clamp(local.y / max(size.y, 1e-6), 0.0, 1.0);
    vec4 fill = mix(v_color, v_color2, t);
    if (v_info.x != 0u) {
        // Clamp to the quad's own uv rect so atlas neighbours never bleed in.
        vec2 uv = clamp(v_uv, min(v_uv_rect.xy, v_uv_rect.zw), max(v_uv_rect.xy, v_uv_rect.zw));
        vec4 tex = texture(textures[nonuniformEXT(v_info.x - 1u)], uv);
        if (v_info.y == KIND_ALPHA_MASK) {
            fill.a *= tex.r;
        } else {
            fill *= tex;
        }
    }
    float border = v_params.y;
    if (border > 0.0) {
        float inner = min(clamp(0.5 - (sd + border), 0.0, 1.0), outer);
        vec4 b = mix(v_border0, v_border1, t);
        float af = clamp(fill.a, 0.0, 1.0) * inner;
        float ab = clamp(b.a, 0.0, 1.0) * (outer - inner);
        out_color = vec4(fill.rgb * af + b.rgb * ab, af + ab);
    } else {
        float a = clamp(fill.a, 0.0, 1.0) * outer;
        out_color = vec4(fill.rgb * a, a);
    }
}
