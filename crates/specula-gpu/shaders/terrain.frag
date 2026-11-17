#version 460
// Terrain patches, fragment stage (src/terrain.rs, docs/design/TERRAIN.md): the terrain's
// normal map and splat weights, up to four layers (tint, optional albedo and normal textures,
// projected from above and, on steep slopes, triplanar), lit like an opaque mesh (lit_common.glsl:
// sun and shadows, point lights, fog, SSAO's ambient luminance in alpha). Compile with
// shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#extension GL_EXT_control_flow_attributes : require
#define GI_FIELDS
#include "frame.glsl"
#include "terrain.glsl"

// Must match `PushConstants` in src/render.rs (the fifth field: the terrain records).
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;
    uint material;
    uint pass;
    Terrains terrains;
} pc;

#include "lit_common.glsl"
#include "form.glsl"

layout(location = 0) in vec3 v_world;
layout(location = 1) in vec2 v_cell;
layout(location = 2) flat in uint v_terrain;
layout(location = 3) flat in vec3 v_cof0;
layout(location = 4) flat in vec3 v_cof1;
layout(location = 5) flat in vec3 v_cof2;
layout(location = 6) in vec3 v_gi; // tier low's ambient, per vertex (terrain.vert)

layout(location = 0) out vec4 out_color;


// Grain: value noise on a float hash ("hash without sine", Dave Hoskins): integer multiplies are
// slow on the iGPU, and grain needs no bit-exact hash (the same GPU repeats it exactly).
float grain_hash(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

float grain_noise(vec2 x) {
    vec2 i = floor(x);
    vec2 f = fract(x);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(grain_hash(i), grain_hash(i + vec2(1.0, 0.0)), f.x), mix(grain_hash(i + vec2(0.0, 1.0)), grain_hash(i + vec2(1.0)), f.x), f.y);
}

// A layer texture projected along one axis, with explicit gradients (layers are sampled in
// non-uniform control flow, where implicit derivatives are undefined).
vec4 project(uint slot, vec2 uv, vec2 dx, vec2 dy) {
    return textureGrad(textures[nonuniformEXT(slot - 1u)], uv, dx, dy);
}

// Layer `slot` at world `p`: from above, blending in the X and Z projections on slopes steeper
// than the layer's threshold (`cos_thr`: cosine of that slope; < 0 = top only).
vec4 layer_texture(uint slot, vec3 p, vec3 n, float inv_scale, float cos_thr, vec3 dpx, vec3 dpy) {
    vec4 top = project(slot, p.xz * inv_scale, dpx.xz * inv_scale, dpy.xz * inv_scale);
    if (cos_thr < 0.0) {
        return top;
    }
    vec3 an = abs(n);
    // Fully top until the threshold, fully triplanar 0.15 below it in cosine.
    float s = smoothstep(cos_thr, cos_thr - 0.15, an.y);
    if (s <= 0.0) {
        return top;
    }
    vec3 wgt = an * an;
    wgt *= wgt;
    wgt /= max(wgt.x + wgt.y + wgt.z, 1e-6);
    vec4 tri = top * wgt.y;
    float used = wgt.y;
    if (wgt.x > 0.02) {
        tri += project(slot, p.zy * inv_scale, dpx.zy * inv_scale, dpy.zy * inv_scale) * wgt.x;
        used += wgt.x;
    }
    if (wgt.z > 0.02) {
        tri += project(slot, p.xy * inv_scale, dpx.xy * inv_scale, dpy.xy * inv_scale) * wgt.z;
        used += wgt.z;
    }
    return mix(top, tri / max(used, 1e-6), s);
}

void main() {
    // Fields read one by one through the reference (a whole 256-byte record per pixel would
    // cost registers and bandwidth on the iGPU).
    uvec4 size = pc.terrains.items[v_terrain].size;
    uvec4 more = pc.terrains.items[v_terrain].more;
    float grain = pc.terrains.items[v_terrain].look.x;
    vec2 uv = (v_cell + 0.5) / vec2(size.xy);
    // The terrain's normal (local) into the world: the model's inverse transpose (cofactors).
    vec3 nl = texture(textures[nonuniformEXT(size.w - 1u)], uv).xyz * 2.0 - 1.0;
    vec3 n = normalize(mat3(v_cof0, v_cof1, v_cof2) * nl);
    vec4 splat = texture(textures[nonuniformEXT(more.x - 1u)], uv);
    vec3 dpx = dFdx(v_world);
    vec3 dpy = dFdy(v_world);
    float weights[4] = float[](splat.r, splat.g, splat.b, splat.a);
    uint count = min(more.y, 4u);
    vec3 albedo = vec3(0.0);
    float rough = 0.0;
    float spec = 0.0;
    float total = 0.0;
    vec2 bend = vec2(0.0);
    // Grain for untextured layers: one octave of world-space value noise per pixel, shared by the
    // layers (per layer and two octaves cost the iGPU a millisecond at 1600x900).
    float g = grain > 0.0 ? grain_noise(v_world.xz * 1.3) : 0.5;
    float grain_k = 1.0 + grain * (g - 0.5) * 0.6;
    // Detail near the camera, where a pixel covers a few centimetres: a finer octave, and the
    // splat's 1-sample bilinear blends broken into patches (magnified, they read as blur).
    float detail_k = 1.0;
    float fp = max(length(dpx), length(dpy));
    float detail = grain > 0.0 ? 1.0 - smoothstep(0.04, 0.2, fp) : 0.0;
    if (detail > 0.0) {
        float g2 = grain_noise(v_world.xz * 11.3 + 17.0) * 0.6 + grain_noise(v_world.zx * 3.7 + 5.0) * 0.4;
        detail_k = 1.0 + detail * max(grain, 0.5) * (g2 - 0.5) * 1.3;
        float h = (g2 - 0.5) * 2.2 * detail;
        weights[0] *= 1.0 + h;
        weights[1] *= 1.0 - h;
        weights[2] *= 1.0 + h;
        weights[3] *= 1.0 - h;
    }
    // Unrolled: the layer index is then a constant (a dynamically indexed local array or record
    // field spills to scratch memory on the iGPU).
    [[unroll]] for (uint k = 0u; k < 4u; k++) {
        float w = k < count ? weights[k] : 0.0;
        if (w < 1.0 / 256.0) {
            continue;
        }
        vec4 tint_rough = pc.terrains.items[v_terrain].layers[k].tint_rough;
        vec4 params = pc.terrains.items[v_terrain].layers[k].params;
        uvec2 tex = pc.terrains.items[v_terrain].layers[k].tex.xy;
        vec3 c = tint_rough.rgb;
        if (tex.x != 0u) {
            c *= layer_texture(tex.x, v_world, n, params.y, params.z, dpx, dpy).rgb;
        } else {
            c *= grain_k;
        }
        if (tex.y != 0u) {
            // The layer's normal map from above, bent into the terrain normal (UDN).
            float is = params.y;
            vec3 tn = project(tex.y, v_world.xz * is, dpx.xz * is, dpy.xz * is).xyz * 2.0 - 1.0;
            bend += tn.xy * params.w * w;
        }
        albedo += c * (w * detail_k);
        rough += tint_rough.a * w;
        spec += params.x * w;
        total += w;
    }
    float inv = 1.0 / max(total, 1e-4);
    albedo *= inv;
    rough = clamp(rough * inv, 0.02, 1.0);
    spec *= inv;
    if (bend != vec2(0.0)) {
        n = normalize(n + vec3(bend.x, 0.0, bend.y) * inv);
    }
    // No flip towards the viewer (as meshes do for open shapes): along ridge silhouettes the smooth
    // normal faces away from the camera while the triangle faces it, and a flipped normal paints
    // the ridge black.
    vec3 v = normalize(camera_pos() - v_world);
    if (pc.frame.sky_params.z > 0.5) {
        // A form view (sight_frame form): the ground as shape alone, as meshes show theirs.
        out_color = form_view(pc.frame.sky_params.z, n, v, v_world);
        return;
    }
    vec3 diffuse, specular;
    float sun;
    // Baked GI: per vertex at tier low (terrain.vert), per pixel at high, the flat ambient without.
    vec3 amb = (GI && !GI_PIXEL) ? v_gi : lit_ambient(v_world, n, v);
    if (gi_irradiance_view()) {
        // sight_frame gi: "irradiance": the indirect light alone, as meshes show it.
        vec3 shown = gi_irradiance_shown(amb);
        out_color = vec4(display(shown), SSAO && HDR ? clamp(dot(shown, LUMA) * pc.frame.ambient.w, 0.0, 60000.0) : 1.0);
        return;
    }
    gather_light(v_world, n, v, rough, amb, diffuse, specular, sun);
    vec3 c = albedo * diffuse;
    vec3 ambient = SSAO ? albedo * amb : vec3(0.0);
    if (spec > 0.0) {
        float mm = 1.0 - max(dot(n, v), 0.0);
        float m2 = mm * mm;
        float fresnel = 0.04 + 0.96 * (m2 * m2 * mm);
        // Baked GI dims what the surroundings reflect where they are dark (a cave, a crypt).
        vec3 reflected = mix(env_radiance(reflect(-v, n)) * gi_occlusion(amb), amb, rough);
        c += spec * fresnel * (specular + reflected * (1.0 - 0.7 * rough));
    }
    if (FOG) {
        float tr;
        c = fogged_at(v_world, c, 1.0, tr);
        ambient *= tr;
    }
    if (SSAO && HDR) {
        out_color = vec4(display(c), clamp(dot(ambient, LUMA) * pc.frame.ambient.w, 0.0, 60000.0));
    } else {
        out_color = vec4(display(c), 1.0);
    }
}
