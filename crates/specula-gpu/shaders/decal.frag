#version 460
// Decals, fragment stage (src/effects.rs, docs/design/VFX.md): the opaque surface under the
// pixel, reconstructed from the scene's depth (any depth convention: the inverse view-projection
// does it), moved into the decal's box; outside it nothing is drawn. Inside, the texture at the
// box's XZ: re-lit (albedo, optional normal map, roughness, specular, emissive) and blended over
// the surface, or (MULTIPLY) multiplied into the surface's colour. Surfaces turned away from the
// box's up axis fade out (`angle_fade`), and so do the box's top and bottom ends.
// Compile with shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#define GI_FIELDS
#include "decal.glsl"
#define SUN_SHADOW
layout(set = 0, binding = 0) uniform sampler2DArrayShadow shadow_map;
layout(set = 0, binding = 1) uniform sampler2D textures[];
#include "effects_light.glsl"
#include "gi.glsl"

layout(location = 0) flat in vec4 v_inv0;
layout(location = 1) flat in vec4 v_inv1;
layout(location = 2) flat in vec4 v_inv2;
layout(location = 3) flat in vec4 v_inv3;
layout(location = 4) flat in vec4 v_up;
layout(location = 5) flat in vec4 v_color;
layout(location = 6) flat in vec4 v_uv;
layout(location = 7) flat in vec4 v_params;
layout(location = 8) flat in uvec4 v_info;
layout(location = 9) flat in vec3 v_axis_x;

layout(location = 0) out vec4 out_color;

ivec2 g_size;

float depth_at(ivec2 p) {
    return texelFetch(textures[pc.depth], clamp(p, ivec2(0), g_size - 1), 0).r;
}

bool is_sky(float d) {
    return (pc.mode & 1u) != 0u ? d <= 0.0 : d >= 1.0;
}

vec3 world_at(ivec2 p, float d) {
    vec2 ndc = (vec2(p) + 0.5) / vec2(g_size) * 2.0 - 1.0;
    vec4 w = pc.frame.inv_view_proj * vec4(ndc, d, 1.0);
    return w.xyz / w.w;
}

// The surface's normal from its depth: of each pair of neighbours the one nearer in depth (so
// edges don't bend it), facing the camera.
vec3 depth_normal(ivec2 p, float d, vec3 here) {
    float l = depth_at(p - ivec2(1, 0));
    float r = depth_at(p + ivec2(1, 0));
    float u = depth_at(p - ivec2(0, 1));
    float dn = depth_at(p + ivec2(0, 1));
    vec3 dx = abs(r - d) < abs(d - l) ? world_at(p + ivec2(1, 0), r) - here : here - world_at(p - ivec2(1, 0), l);
    vec3 dy = abs(dn - d) < abs(d - u) ? world_at(p + ivec2(0, 1), dn) - here : here - world_at(p - ivec2(0, 1), u);
    vec3 n = cross(dx, dy);
    float len = length(n);
    n = len > 1e-12 ? n / len : v_up.xyz;
    return dot(n, pc.cam_pos.xyz - here) < 0.0 ? -n : n;
}

void main() {
    g_size = textureSize(textures[pc.depth], 0);
    ivec2 px = ivec2(gl_FragCoord.xy);
    float d = depth_at(px);
    vec3 p = world_at(px, d);
    mat4 inv = mat4(v_inv0, v_inv1, v_inv2, v_inv3);
    vec3 local = (inv * vec4(p, 1.0)).xyz;
    // Texture coordinates across the box's XZ (+Z down the image), into the rect.
    vec2 uv01 = local.xz + 0.5;
    vec2 uv = mix(v_uv.xy, v_uv.zw, uv01);
    // Mip selection from the footprint; big jumps (the edge of a surface) are clamped so they
    // don't blur the pixels next to them.
    vec2 gx = clamp(dFdx(uv), vec2(-0.02), vec2(0.02));
    vec2 gy = clamp(dFdy(uv), vec2(-0.02), vec2(0.02));
    vec4 tex = v_info.x != 0u ? textureGrad(textures[nonuniformEXT(v_info.x - 1u)], uv, gx, gy) : vec4(1.0);
    vec3 nmap = v_info.y != 0u ? textureGrad(textures[nonuniformEXT(v_info.y - 1u)], uv, gx, gy).xyz * 2.0 - 1.0 : vec3(0.0, 0.0, 1.0);
    bool inside = all(lessThanEqual(abs(local), vec3(0.5)));
    if (is_sky(d) || !inside) {
        discard;
    }
    vec3 n = depth_normal(px, d, p);
    float facing = smoothstep(v_up.w, v_up.w + 0.2, dot(n, v_up.xyz));
    float ends = 1.0 - smoothstep(0.35, 0.5, abs(local.y));
    float a = clamp(tex.a * v_color.a * facing * ends, 0.0, 1.0);
    if (a < 1.0 / 512.0) {
        discard;
    }
    float fog_t = 1.0;
    vec3 fog_c = vec3(0.0);
    if (FOG) {
        vec3 dv = p - pc.frame.camera_time.xyz;
        float dist = length(dv);
        vec3 dir = dv / max(dist, 1e-6);
        fog_t = fog_transmittance(pc.frame, dir, dist, false);
        fog_c = fog_radiance(pc.frame, dir);
    }
    vec3 tint = tex.rgb * v_color.rgb;
    if (MULTIPLY) {
        // dst × m (the blend state); far away the fog's colour isn't darkened.
        vec3 m = mix(vec3(1.0), tint, a);
        m = mix(vec3(1.0), m, fog_t);
        const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);
        out_color = vec4(m, SSAO ? dot(m, LUMA) : 1.0);
        return;
    }
    // The decal's own normal: its map in the box's frame on the surface.
    vec3 t = v_axis_x - n * dot(v_axis_x, n);
    t = dot(t, t) > 1e-8 ? normalize(t) : normalize(cross(n, abs(n.y) < 0.99 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0)));
    vec3 b = cross(n, t);
    float k = v_params.w;
    // +Y of a normal map is the image's top; the image's rows run along +Z, so -B is its up.
    vec3 sn = normalize(t * (nmap.x * k) - b * (nmap.y * k) + n * max(nmap.z, 1e-3));
    vec3 view = normalize(pc.cam_pos.xyz - p);
    vec3 diffuse = gi_ambient(p, n, view);
    float ndl = max(dot(sn, pc.frame.sun_dir.xyz), 0.0);
    float shadow = 1.0;
    if (SHADOWS && ndl > 0.0) {
        shadow = sun_shadow(p, n);
    }
    diffuse += pc.frame.sun_color.rgb * ndl * shadow;
    diffuse += point_lights(p, sn);
    vec3 c = tint * diffuse;
    float spec = v_params.y;
    if (spec > 0.0) {
        float rough = clamp(v_params.x, 0.03, 1.0);
        float power = 2.0 / (rough * rough * rough * rough) - 2.0;
        vec3 h = normalize(view + pc.frame.sun_dir.xyz);
        c += pc.frame.sun_color.rgb * (spec * shadow * pow(max(dot(sn, h), 0.0), max(power, 1.0)) * (power + 8.0) / 25.0) * ndl;
    }
    c += tint * v_params.z;
    c = c * fog_t + fog_c * (1.0 - fog_t);
    out_color = vec4(display(c) * a, a);
}
