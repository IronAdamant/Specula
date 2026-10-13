#version 460
// Specula mesh pipeline, fragment stage: every material, chosen by `pc.material`.
// Recompile by hand (no build scripts):
//   glslc -O --target-env=vulkan1.3 mesh.frag -o mesh.frag.spv
//   spirv-val --target-env vulkan1.3 mesh.frag.spv
#extension GL_EXT_buffer_reference : require

// Per-frame data; must match `FrameData` in src/render.rs (and mesh.vert).
struct PointLight {
    vec4 pos_range; // xyz world position, w range (m)
    vec4 color;     // linear RGB x intensity
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Frame {
    mat4 view_proj;
    vec4 camera_time; // xyz camera, w time (s)
    vec4 sun_dir;     // xyz towards the sun
    vec4 sun_color;   // linear RGB x intensity
    vec4 ambient;     // linear RGB x intensity, w exposure
    vec4 sky;         // linear clear colour, w tone mapping on
    vec4 viewport;    // width, height, light count
    PointLight lights[16];
};

// Per-draw records; must match `DrawRecord` in src/render.rs (and mesh.vert).
struct Draw {
    mat4 model;  // object -> world
    vec4 color;  // linear RGB, a opacity
    vec4 color2; // linear RGB, a seed
    vec4 params; // emissive, metallic, roughness, specular
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Draws {
    Draw items[];
};

// Set once per instanced batch; must match `PushConstants` in src/render.rs.
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;
    uint material; // the same for every instance of a batch, so branches on it stay uniform
    uint pass;
} pc;

layout(location = 0) in vec3 v_world;
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec3 v_local;
layout(location = 3) flat in uint v_draw;

layout(location = 0) out vec4 out_color;

// This fragment's draw record.
#define DRAW pc.draws.items[v_draw]

const uint MAT_LIT = 0u;
const uint MAT_GLASS = 1u;
const uint MAT_BLOB_SHADOW = 2u;
const uint MAT_LAVA_LAMP = 3u;
const float PI = 3.14159265;

// ---------------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------------

vec3 camera_pos() { return pc.frame.camera_time.xyz; }

// Integer hashes (PCG): bit-exact on every GPU, so a seed gives the same lamp everywhere.
uint pcg(uint v) {
    uint state = v * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

// Uniform in [0, 1) from integer coordinates.
float hash_u(uint a, uint b) { return float(pcg(a + pcg(b)) >> 8) / 16777216.0; }
float hash_i3(ivec3 c) { return float(pcg(uint(c.x) + pcg(uint(c.y) + pcg(uint(c.z)))) >> 8) / 16777216.0; }

float value_noise(vec3 x) {
    ivec3 i = ivec3(floor(x));
    vec3 f = fract(x);
    f = f * f * (3.0 - 2.0 * f);
    const ivec2 o = ivec2(0, 1);
    return mix(mix(mix(hash_i3(i + o.xxx), hash_i3(i + o.yxx), f.x), mix(hash_i3(i + o.xyx), hash_i3(i + o.yyx), f.x), f.y),
               mix(mix(hash_i3(i + o.xxy), hash_i3(i + o.yxy), f.x), mix(hash_i3(i + o.xyy), hash_i3(i + o.yyy), f.x), f.y),
               f.z);
}

// Polynomial smooth minimum: blends two distance fields over width k.
float smin(float a, float b, float k) {
    float h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

// ACES filmic curve (Narkowicz fit).
vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

// Linear scene light -> value written to the sRGB target. Without tone mapping this is the M0
// path (exposure 1, clamp on write).
vec3 display(vec3 c) {
    c *= pc.frame.ambient.w;
    if (pc.frame.sky.w > 0.5) {
        c = aces(c);
        // Dither by about one sRGB step so dark gradients don't band.
        float n = hash_u(uint(gl_FragCoord.x), uint(gl_FragCoord.y)) - 0.5;
        c += n * 2.2 * pow(max(c, vec3(1e-4)), vec3(0.545)) / 255.0;
    }
    return max(c, vec3(0.0));
}

// What the surroundings reflect: the ambient light, brighter from above, a hint of sky, and a
// soft glow around the sun (as a window or the moon would show in a glossy surface).
vec3 env_radiance(vec3 dir) {
    float up = clamp(dir.y * 0.5 + 0.5, 0.0, 1.0);
    float sun = pow(max(dot(dir, pc.frame.sun_dir.xyz), 0.0), 24.0);
    return pc.frame.ambient.rgb * mix(0.35, 1.25, up) + pc.frame.sky.rgb * 0.25 * up + pc.frame.sun_color.rgb * 4.0 * sun;
}

// GGX distribution x Smith visibility (without Fresnel).
float ggx(vec3 n, vec3 v, vec3 l, float rough) {
    vec3 h = normalize(v + l);
    float a = max(rough * rough, 0.002);
    float a2 = a * a;
    float ndh = max(dot(n, h), 0.0);
    float ndv = max(dot(n, v), 1e-4);
    float ndl = max(dot(n, l), 1e-4);
    float den = ndh * ndh * (a2 - 1.0) + 1.0;
    float k = a * 0.5;
    return a2 / (PI * den * den) * 0.25 / ((ndv * (1.0 - k) + k) * (ndl * (1.0 - k) + k));
}

// Diffuse irradiance (ambient + sun + point lights) and specular radiance at a surface.
void gather_light(vec3 p, vec3 n, vec3 v, float rough, out vec3 diffuse, out vec3 specular) {
    diffuse = pc.frame.ambient.rgb;
    specular = vec3(0.0);
    vec3 l = pc.frame.sun_dir.xyz;
    float ndl = max(dot(n, l), 0.0);
    diffuse += pc.frame.sun_color.rgb * ndl;
    specular += pc.frame.sun_color.rgb * ndl * ggx(n, v, l, rough);
    int count = int(pc.frame.viewport.z);
    for (int i = 0; i < count; i++) {
        vec3 to_light = pc.frame.lights[i].pos_range.xyz - p;
        float d2 = dot(to_light, to_light);
        float d = sqrt(d2);
        float x = d / pc.frame.lights[i].pos_range.w;
        float window = clamp(1.0 - x * x * x * x, 0.0, 1.0);
        vec3 radiance = pc.frame.lights[i].color.rgb * window * window / (d2 + 0.0025);
        vec3 ld = to_light / max(d, 1e-5);
        float nl = max(dot(n, ld), 0.0);
        diffuse += radiance * nl;
        specular += radiance * nl * ggx(n, v, ld, rough);
    }
}

// ---------------------------------------------------------------------------------------------
// Lit: opaque surfaces
// ---------------------------------------------------------------------------------------------

vec4 shade_lit() {
    vec3 albedo = DRAW.color.rgb;
    float emissive = DRAW.params.x;
    float metallic = DRAW.params.y;
    float rough = clamp(DRAW.params.z, 0.02, 1.0);
    float spec_weight = DRAW.params.w;
    vec3 n = normalize(v_normal);
    vec3 v = normalize(camera_pos() - v_world);
    if (dot(n, v) < 0.0) {
        n = -n; // the inside of an open shape
    }
    vec3 diffuse, specular;
    gather_light(v_world, n, v, rough, diffuse, specular);
    vec3 c = albedo * (1.0 - metallic) * diffuse;
    if (spec_weight > 0.0) {
        vec3 f0 = mix(vec3(0.04), albedo, metallic);
        vec3 fresnel = f0 + (1.0 - f0) * pow(1.0 - max(dot(n, v), 0.0), 5.0);
        vec3 reflected = mix(env_radiance(reflect(-v, n)), pc.frame.ambient.rgb, rough);
        c += spec_weight * fresnel * (specular + reflected * (1.0 - 0.7 * rough));
    }
    c += albedo * emissive;
    return vec4(display(c), 1.0);
}

// ---------------------------------------------------------------------------------------------
// Glass: thin blended shells, drawn back faces (pass 0) then front faces (pass 1)
// ---------------------------------------------------------------------------------------------

vec4 shade_glass() {
    vec3 n = normalize(v_normal);
    vec3 v = normalize(camera_pos() - v_world);
    float facing = dot(n, v);
    // Choose faces by their normal, not their winding: works for any mesh.
    if ((pc.pass == 1u) != (facing > 0.0)) {
        discard;
    }
    if (facing < 0.0) {
        n = -n;
    }
    float ndv = clamp(dot(n, v), 0.0, 1.0);
    float fresnel = 0.04 + 0.96 * pow(1.0 - ndv, 5.0);
    vec3 diffuse, specular;
    gather_light(v_world, n, v, 0.05, diffuse, specular);
    vec3 reflected = env_radiance(reflect(-v, n)) + specular;
    // A tinted body: the path through a thin shell grows at grazing angles.
    float body = clamp(DRAW.color.a, 0.0, 1.0);
    float path = 1.0 - pow(1.0 - body, 1.0 / max(ndv, 0.2));
    vec3 body_light = DRAW.color.rgb * (0.5 * diffuse + DRAW.params.x);
    float alpha = path + fresnel * (1.0 - path);
    vec3 c = body_light * path * (1.0 - fresnel) + reflected * fresnel;
    return vec4(display(c), alpha);
}

// ---------------------------------------------------------------------------------------------
// Blob shadow: a soft dark disc on the XZ quad
// ---------------------------------------------------------------------------------------------

vec4 shade_blob_shadow() {
    float r = length(v_local.xz) * 2.0;
    float a = clamp(DRAW.color.a, 0.0, 1.0) * pow(1.0 - smoothstep(0.0, 1.0, r), 1.5);
    return vec4(0.0, 0.0, 0.0, a);
}

// ---------------------------------------------------------------------------------------------
// Lava lamp: ray-marched glass vessel, coloured liquid and moving wax, in the proxy box
// ---------------------------------------------------------------------------------------------
//
// Lamp space = the entity's local axes in metres, origin at the box centre. The vessel fills the
// box height; its widest radius (30% up) is min(width, depth) / 3.2, leaving room for the halo.

float g_h;     // vessel height
float g_rmax;  // widest vessel radius
float g_time;
uint g_seed;   // bits of the seed

float vessel_radius(float y) {
    float t = clamp(y / g_h + 0.5, 0.0, 1.0);
    // Two cones meeting 30% up, the shoulder rounded like blown glass.
    float lower = mix(0.62, 1.0, t / 0.3);
    float upper = mix(1.0, 0.42, (t - 0.3) / 0.7);
    return g_rmax * smin(lower, upper, 0.06);
}

// Distance bound to the vessel's inner surface (negative inside).
float vessel(vec3 p) {
    float side = (length(p.xz) - vessel_radius(p.y)) * 0.95;
    return max(side, abs(p.y) - 0.5 * g_h);
}

vec3 vessel_normal(vec3 p) {
    vec2 e = vec2(1e-3 * g_h, 0.0);
    return normalize(vec3(vessel(p + e.xyy) - vessel(p - e.xyy),
                          vessel(p + e.yxy) - vessel(p - e.yxy),
                          vessel(p + e.yyx) - vessel(p - e.yyx)));
}

// Distance bound to the wax: a pool on the bottom, a thin layer under the cap, and blobs that
// rise when heated, cool, and sink, merging and splitting on the way.
float wax(vec3 p) {
    float yb = -0.5 * g_h;
    float yt = 0.5 * g_h;
    float t = g_time;
    float heave = 0.012 * g_h * sin(3.0 * p.x / g_rmax + 0.7 * t) * sin(2.0 * p.z / g_rmax - 0.5 * t);
    float d = p.y - (yb + 0.07 * g_h + heave);
    d = min(d, (yt - 0.035 * g_h) - p.y);
    for (int i = 0; i < 6; i++) {
        uint k = g_seed ^ (uint(i) * 16u);
        float h1 = hash_u(k, 1u);
        float h2 = hash_u(k, 2u);
        float h3 = hash_u(k, 3u);
        float h4 = hash_u(k, 4u);
        float period = 22.0 + 16.0 * h1;
        float phase = t / period + h2;
        float s = 0.5 - 0.5 * cos(2.0 * PI * phase); // 0 at the bottom, 1 at the top
        float rising = sin(2.0 * PI * phase);
        float y = mix(yb + 0.10 * g_h, yt - 0.12 * g_h, s);
        float room = vessel_radius(y);
        vec2 drift = vec2(sin(t * 0.21 * (0.6 + h3) + 6.28 * h4), cos(t * 0.17 * (0.6 + h4) + 6.28 * h3)) * 0.45 * room;
        float radius = g_rmax * (0.16 + 0.20 * h3);
        float stretch = 1.0 + 0.55 * abs(rising); // elongated while moving
        vec3 radii = vec3(radius / sqrt(stretch), radius * stretch, radius / sqrt(stretch));
        vec3 q = p - vec3(drift.x, y, drift.y);
        float blob = (length(q / radii) - 1.0) * min(radii.x, radii.y);
        d = smin(d, blob, 0.22 * g_rmax);
    }
    // Uneven, slowly churning surfaces.
    d += 0.008 * g_h * (value_noise(p * (5.0 / g_rmax) + vec3(0.0, 0.2 * t, 0.0)) - 0.5);
    return d;
}

vec3 wax_normal(vec3 p) {
    vec2 k = vec2(1.0, -1.0);
    float e = 2e-3 * g_h;
    return normalize(k.xyy * wax(p + k.xyy * e) + k.yyx * wax(p + k.yyx * e) + k.yxy * wax(p + k.yxy * e) +
                     k.xxx * wax(p + k.xxx * e));
}

vec4 shade_lava_lamp() {
    mat4 m = DRAW.model;
    vec3 scale = vec3(length(m[0].xyz), length(m[1].xyz), length(m[2].xyz));
    mat3 rot = mat3(m[0].xyz / scale.x, m[1].xyz / scale.y, m[2].xyz / scale.z);
    vec3 origin = m[3].xyz;
    g_h = scale.y;
    g_rmax = 0.5 * min(scale.x, scale.z) / 1.6;
    g_time = pc.frame.camera_time.w;
    g_seed = floatBitsToUint(DRAW.color2.a);

    vec3 ro = transpose(rot) * (camera_pos() - origin);
    vec3 half_size = 0.5 * scale;
    // Shade each pixel once: on the proxy's near faces, so opaque parts inside the box (a lamp
    // base) don't hide the lamp; on its far faces when the camera is inside the box.
    bool inside_box = all(lessThan(abs(ro), half_size));
    bool near_face = dot(normalize(v_normal), camera_pos() - v_world) > 0.0;
    if (near_face == inside_box) {
        discard;
    }
    vec3 rd = normalize(v_local * scale - ro);
    vec3 inv = 1.0 / rd;
    vec3 t0 = (-half_size - ro) * inv;
    vec3 t1 = (half_size - ro) * inv;
    vec3 tmin = min(t0, t1);
    vec3 tmax = max(t0, t1);
    float t_near = max(max(tmin.x, tmin.y), max(tmin.z, 0.0));
    float t_far = min(min(tmax.x, tmax.y), tmax.z);

    vec3 wax_color = DRAW.color.rgb;
    vec3 liquid = DRAW.color2.rgb;
    float glow = DRAW.params.x;

    // Halo: glare around the bright core, by the ray's closest approach to the lamp's axis.
    vec2 o2 = ro.xz;
    vec2 d2 = rd.xz;
    float dd = dot(d2, d2);
    float th = dd > 1e-8 ? max(-dot(o2, d2) / dd, 0.0) : 0.0;
    vec3 closest = ro + rd * th;
    float over = max(abs(closest.y + 0.1 * g_h) - 0.35 * g_h, 0.0);
    float axis_dist = sqrt(dot(closest.xz, closest.xz) + over * over);
    float fade = clamp(1.0 - axis_dist / (0.5 * min(scale.x, scale.z)), 0.0, 1.0);
    vec3 halo = mix(wax_color, liquid, 0.4) * glow * fade * fade *
                (0.05 * exp(-pow(axis_dist / (1.2 * g_rmax), 2.0)) + 0.03 * exp(-axis_dist / (0.9 * g_rmax)));

    // Find the glass.
    float t = t_near;
    bool hit = false;
    for (int i = 0; i < 48; i++) {
        float d = vessel(ro + rd * t);
        if (d < 1e-4 * g_h) {
            hit = true;
            break;
        }
        t += d;
        if (t > t_far) {
            break;
        }
    }
    if (!hit) {
        return vec4(display(halo), 0.0);
    }

    vec3 p = ro + rd * t;
    vec3 n = vessel_normal(p);
    float cos_i = clamp(dot(-rd, n), 0.0, 1.0);
    float fresnel = 0.04 + 0.96 * pow(1.0 - cos_i, 5.0);

    // Reflections and highlights on the glass, lit by the scene (world space).
    vec3 pw = origin + rot * p;
    vec3 nw = normalize(rot * n);
    vec3 vw = normalize(camera_pos() - pw);
    vec3 diffuse, specular;
    gather_light(pw, nw, vw, 0.04, diffuse, specular);
    vec3 reflected = env_radiance(reflect(-vw, nw)) + specular;

    // Refract into the liquid (the thin glass wall barely bends the ray on its own).
    vec3 rd2 = refract(rd, n, 1.0 / 1.33);
    if (dot(rd2, rd2) < 1e-6) {
        rd2 = reflect(rd, n);
    }
    vec3 bulb = vec3(0.0, -0.53 * g_h, 0.0);
    vec3 sigma = (vec3(1.0) - 0.9 * liquid) * (1.4 / g_rmax) + 0.06 / g_rmax; // absorption per metre
    vec3 transmit = vec3(1.0);
    vec3 inside = vec3(0.0);
    float s = 0.004 * g_h;
    for (int i = 0; i < 96; i++) {
        vec3 q = p + rd2 * s;
        if (vessel(q) > 0.0) {
            break; // out through the far wall
        }
        float dw = wax(q);
        if (dw < 6e-4 * g_h) {
            // Wax: glows from the heat below, translucent where thin, with wet highlights.
            vec3 nx = wax_normal(q);
            float height = clamp(q.y / g_h + 0.5, 0.0, 1.0);
            float heat = exp(-3.0 * height);
            vec3 to_bulb = normalize(bulb - q);
            float wrap = max((dot(nx, to_bulb) + 0.5) / 1.5, 0.0);
            float thick = clamp(-wax(q - nx * 0.12 * g_rmax) / (0.12 * g_rmax), 0.0, 1.0);
            float rim = pow(1.0 - clamp(dot(nx, -rd2), 0.0, 1.0), 2.5);
            // Thick wax absorbs more of its own glow: deeper, more saturated cores.
            vec3 body = mix(wax_color, wax_color * wax_color * 1.4, 0.7 * thick);
            vec3 c = body * glow * (0.22 + 1.3 * heat) * (0.45 + 0.55 * wrap);
            c += wax_color * glow * (0.9 * (1.0 - thick) + 1.1 * rim) * (0.3 + heat);
            c += wax_color * pc.frame.ambient.rgb * 0.3;
            vec3 h = normalize(to_bulb - rd2);
            c += vec3(1.0, 0.9, 0.8) * pow(max(dot(nx, h), 0.0), 16.0) * 0.12 * glow * (0.3 + heat);
            c *= mix(vec3(1.0), vec3(1.2, 1.08, 0.8), heat);
            inside += transmit * c;
            transmit = vec3(0.0);
            break;
        }
        float step_len = clamp(dw * 0.85, 0.003 * g_h, 0.035 * g_h);
        // The liquid scatters the bulb's light (strongest low down) and the wax's glow.
        float height = q.y / g_h + 0.5;
        vec3 scatter = liquid * glow * (0.04 + 0.7 * exp(-4.5 * height)) * (0.4 / g_rmax);
        scatter += wax_color * glow * 0.6 * exp(-max(dw, 0.0) / (0.12 * g_rmax)) * (0.35 / g_rmax);
        vec3 tr = exp(-sigma * step_len);
        inside += transmit * scatter * (1.0 - tr) / sigma;
        transmit *= tr;
        s += step_len;
    }

    // Near the silhouette the ray grazes the thick glass and the liquid bends it away: darker edges.
    inside *= mix(0.55, 1.0, smoothstep(0.0, 0.35, cos_i));
    vec3 c = reflected * fresnel + inside * (1.0 - fresnel) + halo;
    float alpha = fresnel + (1.0 - fresnel) * (1.0 - (transmit.r + transmit.g + transmit.b) / 3.0);
    return vec4(display(c), alpha);
}

void main() {
    if (pc.material == MAT_GLASS) {
        out_color = shade_glass();
    } else if (pc.material == MAT_BLOB_SHADOW) {
        out_color = shade_blob_shadow();
    } else if (pc.material == MAT_LAVA_LAMP) {
        out_color = shade_lava_lamp();
    } else {
        out_color = shade_lit();
    }
}
