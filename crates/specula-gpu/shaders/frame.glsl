// Shared declarations: per-frame data, per-draw records and the global descriptor set.
// Included by the stage sources (glslc resolves #include); not compiled on its own.
#extension GL_EXT_buffer_reference : require

#ifdef GI_FIELDS
#extension GL_EXT_control_flow_attributes : require
// Baked GI volumes and probes (src/gi.rs, shaders/gi.glsl), read through the frame's address.
// Declared only by the stages that sample GI (they define GI_FIELDS first): the others read a
// prefix of the same buffer and compile exactly as before.
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer GiData {
    uvec4 words[];
};
// Per draw record: its GI sampled on the CPU (src/gi.rs `DrawGi`, three words each), then the
// wide draws' grids (two words a sample).
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer GiDraws {
    uvec4 d[];
};
#endif

// Per-frame data; must match `FrameData` in src/render.rs.
struct PointLight {
    vec4 pos_range; // xyz world position, w range (m)
    vec4 color;     // linear RGB x intensity
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Frame {
    mat4 view_proj;
    vec4 camera_time;   // xyz camera, w time (s)
    vec4 sun_dir;       // xyz towards the sun
    vec4 sun_color;     // linear RGB x intensity
    vec4 ambient;       // linear RGB x intensity, w exposure
    vec4 sky;           // linear clear colour, w tone mapping on
    vec4 viewport;      // width, height, light count
    PointLight lights[16];
    mat4 inv_view_proj;
    mat4 shadow_mats[4]; // world -> shadow clip per cascade
    vec4 shadow_params; // cascades, filter radius (texels of cascade 0), normal offset (texels)
    vec4 shadow_texel;  // world size of a texel per cascade
    vec4 shadow_axis;   // view depth = dot(xyz, p) + w
    vec4 shadow_fade;   // view depths where shadows start / finish fading out
    vec4 fog_color;     // linear RGB, w density (per m at the reference height)
    vec4 fog_params;    // falloff (per m), reference height, start distance, sun scatter
    vec4 sky_zenith;    // linear RGB x intensity, w cos(disc radius)
    vec4 sky_horizon;   // linear RGB x intensity, w disc intensity
    vec4 sky_ground;    // linear RGB x intensity, w glow strength
    vec4 sky_params;    // horizon sharpness, y: the sky is drawn (1)
    vec4 wind;          // xy: horizontal direction x strength (world x, z), z: frequency (Hz), w: gust
    vec4 foliage;       // swaying draws fade out from x to y metres from the camera (y = 0: never)
#ifdef GI_FIELDS
    vec4 gi_params;     // GI volumes, visibility on, irradiance view, unused
    GiData gi_data;     // the GI buffer (unused without GI)
    GiDraws gi_draws;   // per-draw GI (gi_params.w = 1), else unused
#endif
};

// Fraction of light that crosses the height fog from `dist` metres along `dir` to the camera
// (`to_infinity` for the sky: then the ray must climb out of the fog or it is fully fogged).
float fog_transmittance(Frame f, vec3 dir, float dist, bool to_infinity) {
    float start = f.fog_params.z;
    if (!to_infinity && dist <= start) {
        return 1.0;
    }
    vec3 o = f.camera_time.xyz + dir * start;
    float density = f.fog_color.w;
    float falloff = f.fog_params.x;
    float k = density * exp(clamp(-falloff * (o.y - f.fog_params.y), -80.0, 80.0));
    float optical;
    if (to_infinity) {
        if (falloff * dir.y <= 1e-5) {
            return 0.0;
        }
        optical = k / (falloff * dir.y);
    } else {
        float len = dist - start;
        float x = falloff * dir.y * len;
        optical = abs(x) > 1e-4 ? k * len * (1.0 - exp(clamp(-x, -80.0, 80.0))) / x : k * len;
    }
    return exp(-optical);
}

// The fog's colour seen along `dir`: brighter towards the sun.
vec3 fog_radiance(Frame f, vec3 dir) {
    float s = max(dot(dir, f.sun_dir.xyz), 0.0);
    s *= s;
    s *= s;
    s *= s; // the 8th power, by multiplication (pow is two transcendentals per pixel)
    return f.fog_color.rgb + f.sun_color.rgb * (f.fog_params.w * s);
}

// Per-draw records; must match `DrawRecord` in src/render.rs.
struct Draw {
    mat4 model;  // object -> world
    vec4 color;  // linear RGB, a opacity
    vec4 color2; // linear RGB, a seed (textured Lit: emissive colour x strength, a normal scale)
    vec4 params; // emissive, metallic, roughness, specular
    uvec4 ext;   // texture slots + 1 (x: base | normal << 16, y: metal-rough | emissive << 16),
                 // z: first bone matrix, w: flags (EXT_*) | alpha cutoff (unorm8) << 8
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Draws {
    Draw items[];
};

// `Draw.ext.w` flags.
const uint EXT_ALPHA_MASK = 1u;
const uint EXT_EMISSIVE_COLOR = 2u;
// A wind term (`wind_bits` in src/render.rs): amount (m, unorm8) << 16, full-weight height
// (1/32 m) << 24; EXT_WIND_PAINTED: the vertex colour's alpha is the weight instead.
const uint EXT_WIND = 4u;
const uint EXT_WIND_PAINTED = 8u;

// Skinning matrices of the frame's skinned draws, palettes one after another.
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Bones {
    mat4 m[];
};

// Skinning matrix of a vertex: its (up to) four bones' matrices by weight.
mat4 skin_matrix(Bones bones, uint first, uvec4 joints, vec4 weights) {
    return bones.m[first + joints.x] * weights.x + bones.m[first + joints.y] * weights.y +
           bones.m[first + joints.z] * weights.z + bones.m[first + joints.w] * weights.w;
}

// The scale a draw sways in: its model's smaller horizontal scale, so the sway never reaches past
// the entity's box grown by `Wind::reach` in object units (src/models.rs `drawn_bounds`).
// y: its vertical scale, which bounds the lean and the tip's drop (a flattened tuft stays above
// its base).
vec2 wind_scale(mat4 m) {
    return vec2(min(length(m[0].xyz), length(m[2].xyz)), length(m[1].xyz));
}

// The world offset the wind sways a vertex by (src/lib.rs `WindSettings`, `Wind`): `flags` the
// draw's ext.w, `local` the object-space position, `world` the world position, `paint` the vertex
// colour's alpha. Deterministic in the frame's time (simulation time): a bend along the wind plus
// two sine trains rolling across the field along it, under a slow gust envelope; at most 1.25 x
// amount x strength (`Wind::reach`). Every stage that draws meshes applies it identically
// (mesh_vert.glsl, shadow_vert.glsl, ids_vert.glsl), so the shadow and the ID buffer move with
// the image.
vec3 wind_offset(Frame f, uint flags, vec3 local, vec3 world, float paint, vec2 scales) {
    float scale = scales.x;
    if ((flags & EXT_WIND) == 0u) {
        return vec3(0.0);
    }
    float strength = length(f.wind.xy);
    if (strength < 1e-5) {
        return vec3(0.0);
    }
    vec2 dir = f.wind.xy / strength;
    float amount = float((flags >> 16) & 255u) / 255.0;
    float height = max(float(flags >> 24) / 32.0, 0.03);
    bool painted = (flags & EXT_WIND_PAINTED) != 0u;
    float k = painted ? clamp(paint, 0.0, 1.0) : clamp(local.y / height, 0.0, 1.0);
    k *= k; // bases stay planted, tips bend the most
    float t = f.camera_time.w;
    float along = dot(world.xz, dir);
    float across = dot(world.xz, vec2(-dir.y, dir.x));
    const float TAU = 6.2831853;
    float phase = TAU * fract(t * f.wind.z) - along * 0.45;
    float wave = 0.6 * sin(phase) + 0.25 * sin(phase * 2.31 + across * 0.9 + 1.7);
    float gust = 1.0 + 0.4 * f.wind.w * sin(TAU * fract(t * 0.07) - along * 0.06);
    // In the draw's own scale (a bigger tuft sways further), never past 0.8 of its height.
    float sway = (0.35 + 0.5 * wave) * gust * strength * amount * k * scale;
    float side = 0.12 * sin(phase * 1.63 + across * 1.3) * strength * amount * k * scale;
    vec2 o = dir * sway + vec2(-dir.y, dir.x) * side;
    float h = max(height * min(scale, scales.y), 0.03);
    float len = length(o);
    o *= len > 0.8 * h ? 0.8 * h / len : 1.0;
    // Bending keeps a blade's length roughly: the tip drops as it leans (by height only: at most
    // a third of the height at the tip, so it stays above the base).
    float drop = painted ? 0.0 : 0.5 * dot(o, o) / h;
    return vec3(o.x, -drop, o.y);
}

// How much of a foliage card's coverage is kept as it turns edge-on to the camera: a swaying,
// alpha-cut draw (`flags` both bits) scales its texture's alpha by this, from 0 when the card's
// geometric normal lies across the view (|n.v| <= 0.08) to 1 from 0.3 on; anything else keeps 1.
// A thin card seen edge-on otherwise draws its blades as dashed lines (review R1). The scene
// (mesh_frag.glsl) and the ID buffer (ids.glsl cut_out) cut the same fragments.
float foliage_edge(uint flags, vec3 normal, vec3 world, vec3 camera) {
    if ((flags & (EXT_WIND | EXT_ALPHA_MASK)) != (EXT_WIND | EXT_ALPHA_MASK)) {
        return 1.0;
    }
    float f = abs(dot(normalize(normal), normalize(camera - world)));
    return smoothstep(0.08, 0.3, f);
}

// Whether a fragment of a swaying draw (foliage) is faded out at its distance: an ordered 4 x 4
// dither between frame.foliage.x and .y metres from the camera, by pixel (deterministic; the ID
// buffer cuts the same pixels: ids_opaque.glsl, ids_count.glsl).
bool foliage_faded(Frame f, uint flags, vec3 world, vec2 frag) {
    if ((flags & EXT_WIND) == 0u || f.foliage.y <= 0.0) {
        return false;
    }
    float t = clamp((distance(world, f.camera_time.xyz) - f.foliage.x) / max(f.foliage.y - f.foliage.x, 1e-3), 0.0, 1.0);
    if (t <= 0.0) {
        return false;
    }
    // Bayer 4 x 4: thresholds 0..15 spread over the block.
    const uint BAYER[16] = uint[](0u, 8u, 2u, 10u, 12u, 4u, 14u, 6u, 3u, 11u, 1u, 9u, 15u, 7u, 13u, 5u);
    uvec2 p = uvec2(frag) & 3u;
    return t > (float(BAYER[p.y * 4u + p.x]) + 0.5) / 16.0;
}
