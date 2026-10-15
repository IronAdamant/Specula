// Included by shape.vert and ids_shape.vert (IDS: sight's ID buffer, the item's id too).
// Vector shapes, vertex stage: a tessellated shape (src/vector.rs) placed by its item's model
// matrix. Edge vertices carry an extrusion (the `normal` attribute): its direction is in the
// shape's plane and its length is in **pixels**, so the anti-aliasing fringe is about one pixel
// wide at any distance and zoom. Compile with shaders/build.sh.
#include "sprite.glsl"

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_extrude; // shape-local direction, length = pixels
layout(location = 2) in vec4 in_color;   // sRGB colour (unorm8), straight alpha (the fringe fades it)

layout(location = 0) out vec4 v_color;   // linear, straight alpha
layout(location = 1) out vec3 v_world;
#ifdef IDS
layout(location = 2) flat out uint v_id; // sight's id of the shape's entity (0: none)
#endif

void main() {
    Item it = pc.items.items[gl_InstanceIndex];
    mat4 model = mat4(it.v1, it.v2, it.v3, it.v4);
    vec3 p = (model * vec4(in_position, 1.0)).xyz;
    float px = length(in_extrude);
    if (px > 0.0) {
        vec3 dir = mat3(model) * (in_extrude / px);
        float len = length(dir);
        if (len > 1e-12) {
            dir /= len;
            // Pixels per metre along `dir` at p: the derivative of the projection.
            vec4 c = pc.frame.view_proj * vec4(p, 1.0);
            vec4 dc = pc.frame.view_proj * vec4(dir, 0.0);
            vec2 dndc = (dc.xy * c.w - c.xy * dc.w) / (c.w * c.w);
            float ppm = length(dndc * 0.5 * pc.frame.viewport.xy);
            // Seen edge-on a pixel spans a lot of plane; the cap only keeps the maths finite
            // (an edge-on fringe covers no pixels anyway).
            p += dir * (px / max(ppm, 1e-4));
        }
    }
    gl_Position = pc.frame.view_proj * vec4(p, 1.0);
    vec3 s = in_color.rgb;
    vec3 lin = mix(s / 12.92, pow((s + 0.055) / 1.055, vec3(2.4)), greaterThan(s, vec3(0.04045)));
    v_color = vec4(lin * it.tint.rgb, in_color.a * it.tint.a);
    v_world = p;
#ifdef IDS
    v_id = pc.idp.ids.items[gl_InstanceIndex];
#endif
}
