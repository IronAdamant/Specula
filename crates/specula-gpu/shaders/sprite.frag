#version 460
// Sprites and tile chunks, fragment stage: the texel (nearest for pixel art, or bilinear kept
// inside the item's own rect so atlas neighbours never bleed in) times the tint, lit or unlit,
// fogged, then cut out (alpha test, depth written) or blended (premultiplied).
// Compile with shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#include "sprite.glsl"

// Features (src/sprite.rs), specialized per pipeline.
layout(constant_id = 0) const uint FEATURES = 0u;
const bool SHADOWS = (FEATURES & 1u) != 0u;
const bool FOG = (FEATURES & 2u) != 0u;
const bool HDR = (FEATURES & 4u) != 0u;
// SSAO (src/ssao.rs): cutouts write the exposed luminance of their ambient light in alpha, as
// the mesh pipeline's Lit does; blended items keep the alpha below (their blend state).
const bool SSAO = (FEATURES & 32u) != 0u;
const bool CUTOUT = (FEATURES & 128u) != 0u;

layout(set = 0, binding = 0) uniform sampler2DArrayShadow shadow_map;
layout(set = 0, binding = 1) uniform sampler2D textures[];

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec3 v_world;
layout(location = 2) flat in vec4 v_tint;
layout(location = 3) flat in uvec4 v_info;
layout(location = 4) flat in vec4 v_rect;   // sprite: texel bounds | tiles: cells x0, y0, x1, y1
layout(location = 5) flat in vec4 v_params; // cutoff | tiles: margin, spacing, columns
layout(location = 6) flat in vec4 v_tile;   // tiles: tile size (px) | 1 / texture size
layout(location = 7) flat in vec3 v_normal;

layout(location = 0) out vec4 out_color;

// Integer hash (PCG), for dithering.
uint pcg(uint v) {
    uint state = v * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}
float hash_u(uint a, uint b) { return float(pcg(a + pcg(b)) >> 8) / 16777216.0; }

vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

// Linear scene light -> the value written: the same as the mesh pipeline's `display`.
vec3 display(vec3 c) {
    c *= pc.frame.ambient.w;
    if (HDR) {
        return clamp(c, vec3(0.0), vec3(60000.0));
    }
    if (pc.frame.sky.w > 0.5) {
        c = aces(c);
        float n = hash_u(uint(gl_FragCoord.x), uint(gl_FragCoord.y)) - 0.5;
        c += n * 2.2 * pow(max(c, vec3(1e-4)), vec3(0.545)) / 255.0;
    }
    return max(c, vec3(0.0));
}

const vec2 POISSON[12] = vec2[](
    vec2(-0.326, -0.406), vec2(-0.840, -0.074), vec2(-0.696, 0.457), vec2(-0.203, 0.621),
    vec2(0.962, -0.195), vec2(0.473, -0.480), vec2(0.519, 0.767), vec2(0.185, -0.893),
    vec2(0.507, 0.064), vec2(0.896, 0.412), vec2(-0.322, -0.933), vec2(-0.792, -0.598));

// The mesh pipeline's sun shadow lookup (see shaders/mesh.frag).
float sun_shadow(vec3 p, vec3 n) {
    float depth = dot(pc.frame.shadow_axis.xyz, p) + pc.frame.shadow_axis.w;
    float fade = smoothstep(pc.frame.shadow_fade.x, pc.frame.shadow_fade.y, depth);
    if (fade >= 1.0) {
        return 1.0;
    }
    float size = float(textureSize(shadow_map, 0).x);
    int count = int(pc.frame.shadow_params.x);
    for (int k = 0; k < count; k++) {
        float texel = pc.frame.shadow_texel[k];
        float radius = max(pc.frame.shadow_params.y * pc.frame.shadow_texel.x / texel, 1.0);
        vec4 s = pc.frame.shadow_mats[k] * vec4(p + n * (texel * pc.frame.shadow_params.z), 1.0);
        vec2 uv = s.xy * 0.5 + 0.5;
        float margin = (radius + 1.5) / size;
        if (all(greaterThan(uv, vec2(margin))) && all(lessThan(uv, vec2(1.0 - margin))) && s.z <= 1.0) {
            float lit = 0.0;
            for (int i = 0; i < 12; i++) {
                lit += texture(shadow_map, vec4(uv + POISSON[i] * (radius / size), float(k), s.z));
            }
            return mix(lit / 12.0, 1.0, fade);
        }
    }
    return 1.0;
}

// Diffuse light on a flat card: ambient, the sun (shadowed) and point lights.
vec3 card_light(vec3 p) {
    vec3 n = normalize(v_normal);
    vec3 v = ortho_view() ? -pc.cam_forward.xyz : normalize(pc.cam_pos.xyz - p);
    if (dot(n, v) < 0.0) {
        n = -n; // the side facing the camera
    }
    vec3 diffuse = pc.frame.ambient.rgb;
    float ndl = max(dot(n, pc.frame.sun_dir.xyz), 0.0);
    if (SHADOWS && ndl > 0.0) {
        ndl *= sun_shadow(p, n);
    }
    diffuse += pc.frame.sun_color.rgb * ndl;
    int count = int(pc.frame.viewport.z);
    for (int i = 0; i < count; i++) {
        vec3 to_light = pc.frame.lights[i].pos_range.xyz - p;
        float d2 = dot(to_light, to_light);
        float d = sqrt(d2);
        float x = d / pc.frame.lights[i].pos_range.w;
        float window = clamp(1.0 - x * x * x * x, 0.0, 1.0);
        vec3 radiance = pc.frame.lights[i].color.rgb * window * window / (d2 + 0.0025);
        diffuse += radiance * max(dot(n, to_light / max(d, 1e-5)), 0.0);
    }
    return diffuse;
}

#include "sprite_texel.glsl"

void main() {
    vec4 texel = v_info.w == KIND_TILES ? tile_texel() : sprite_texel();
    vec3 albedo = texel.rgb * v_tint.rgb;
    float a = clamp(texel.a * v_tint.a, 0.0, 1.0);
    if (CUTOUT) {
        if (a < v_params.x) {
            discard;
        }
        a = 1.0;
    } else if (a <= 0.0) {
        discard;
    }
    bool lit = (v_info.y & FLAG_LIT) != 0u;
    vec3 c = lit ? albedo * card_light(v_world) : albedo;
    // The ambient part (what occlusion may take): none for unlit items.
    vec3 ambient = SSAO && lit ? albedo * pc.frame.ambient.rgb : vec3(0.0);
    if (FOG) {
        vec3 d = v_world - pc.frame.camera_time.xyz;
        float dist = length(d);
        vec3 dir = d / max(dist, 1e-6);
        float t = fog_transmittance(pc.frame, dir, dist, false);
        c = c * t + fog_radiance(pc.frame, dir) * (1.0 - t);
        ambient *= t;
    }
    if (SSAO && HDR && CUTOUT) {
        const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);
        out_color = vec4(display(c), clamp(dot(ambient, LUMA) * pc.frame.ambient.w, 0.0, 60000.0));
        return;
    }
    out_color = vec4(display(c) * a, a);
}
