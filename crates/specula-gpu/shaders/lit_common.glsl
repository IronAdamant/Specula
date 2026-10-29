// Lighting shared by the terrain and water fragment stages (terrain.frag, water.frag): the
// scene's display transform, sun shadows, point lights, the environment reflection and fog, the
// same formulas as mesh_frag.glsl (keep them in step; mesh_frag.glsl keeps its own copy so its
// pipelines stay bit for bit). Included after the stage's push block (`pc.frame`); declares the
// FEATURES constant and the shadow map binding. Not compiled on its own.
//
// GI seam: `lit_ambient(p, n)` is the ambient light at a surface; the baked-GI lane replaces its
// body with a probe lookup (terrain and water both read it through gather_light).

layout(constant_id = 0) const uint FEATURES = 0u;
const bool SHADOWS = (FEATURES & 1u) != 0u;
const bool FOG = (FEATURES & 2u) != 0u;
const bool HDR = (FEATURES & 4u) != 0u;
const bool SSAO = (FEATURES & 32u) != 0u;

layout(set = 0, binding = 0) uniform sampler2DArrayShadow shadow_map;

const float PI = 3.14159265;
const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);

vec3 camera_pos() { return pc.frame.camera_time.xyz; }

// Integer hashes (PCG): bit-exact on every GPU.
uint pcg(uint v) {
    uint state = v * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}
float hash_u(uint a, uint b) { return float(pcg(a + pcg(b)) >> 8) / 16777216.0; }
float hash_i2(ivec2 c) { return float(pcg(uint(c.x) + pcg(uint(c.y))) >> 8) / 16777216.0; }

// Smooth value noise in 2D, 0..1.
float value_noise2(vec2 x) {
    ivec2 i = ivec2(floor(x));
    vec2 f = fract(x);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash_i2(i), hash_i2(i + ivec2(1, 0)), f.x), mix(hash_i2(i + ivec2(0, 1)), hash_i2(i + ivec2(1, 1)), f.x), f.y);
}

// ACES filmic curve (Narkowicz fit).
vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

// Linear scene light -> the value written (mesh_frag.glsl's `display`).
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

// The GI seam: ambient light reaching `p` with normal `n` (today the frame's ambient).
vec3 lit_ambient(vec3 p, vec3 n) {
    return pc.frame.ambient.rgb;
}

// What the surroundings reflect (mesh_frag.glsl's `env_radiance`).
vec3 env_radiance(vec3 dir) {
    float up = clamp(dir.y * 0.5 + 0.5, 0.0, 1.0);
    float sun = max(dot(dir, pc.frame.sun_dir.xyz), 0.0);
    sun *= sun;
    sun *= sun;
    sun *= sun * sun;
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

const vec2 POISSON[12] = vec2[](
    vec2(-0.840, -0.074), vec2(0.962, -0.195), vec2(0.519, 0.767), vec2(0.185, -0.893),
    vec2(-0.326, -0.406), vec2(-0.696, 0.457), vec2(-0.203, 0.621), vec2(0.473, -0.480),
    vec2(0.507, 0.064), vec2(0.896, 0.412), vec2(-0.322, -0.933), vec2(-0.792, -0.598));

// How much sun reaches p (mesh_frag.glsl's `sun_shadow`).
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
            for (int i = 0; i < 4; i++) {
                lit += texture(shadow_map, vec4(uv + POISSON[i] * (radius / size), float(k), s.z));
            }
            if (lit == 0.0 || lit == 4.0) {
                return mix(lit * 0.25, 1.0, fade);
            }
            for (int i = 4; i < 12; i++) {
                lit += texture(shadow_map, vec4(uv + POISSON[i] * (radius / size), float(k), s.z));
            }
            return mix(lit / 12.0, 1.0, fade);
        }
    }
    return 1.0;
}

// Diffuse irradiance and specular radiance at a surface (mesh_frag.glsl's `gather_light`, its
// ambient from `lit_ambient`). `sun` returns the sun's visibility (shadow) for callers that
// reuse it.
void gather_light(vec3 p, vec3 n, vec3 v, float rough, out vec3 diffuse, out vec3 specular, out float sun) {
    diffuse = lit_ambient(p, n);
    specular = vec3(0.0);
    vec3 l = pc.frame.sun_dir.xyz;
    float ndl = max(dot(n, l), 0.0);
    sun = 1.0;
    if (SHADOWS && ndl > 0.0) {
        sun = sun_shadow(p, n);
        ndl *= sun;
    }
    diffuse += pc.frame.sun_color.rgb * ndl;
    specular += pc.frame.sun_color.rgb * ndl * ggx(n, v, l, rough);
    int count = int(pc.frame.viewport.z);
    for (int i = 0; i < count; i++) {
        vec3 to_light = pc.frame.lights[i].pos_range.xyz - p;
        float d2 = dot(to_light, to_light);
        float range = pc.frame.lights[i].pos_range.w;
        if (d2 >= range * range) {
            continue;
        }
        float d = sqrt(d2);
        float x = d / range;
        float window = clamp(1.0 - x * x * x * x, 0.0, 1.0);
        vec3 radiance = pc.frame.lights[i].color.rgb * window * window / (d2 + 0.0025);
        vec3 ld = to_light / max(d, 1e-5);
        float nl = max(dot(n, ld), 0.0);
        diffuse += radiance * nl;
        specular += radiance * nl * ggx(n, v, ld, rough);
    }
}

// Height fog between `p` and the camera over premultiplied light `c` of opacity `alpha`.
vec3 fogged_at(vec3 p, vec3 c, float alpha, out float transmittance) {
    vec3 d = p - camera_pos();
    float dist = length(d);
    vec3 dir = d / max(dist, 1e-6);
    float t = fog_transmittance(pc.frame, dir, dist, false);
    transmittance = t;
    return c * t + fog_radiance(pc.frame, dir) * ((1.0 - t) * alpha);
}
