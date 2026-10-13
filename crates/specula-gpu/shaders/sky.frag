#version 460
// Specula sky, fragment stage: the gradient, the sun/moon disc and its glow, height fog, then the
// same exposure / tone mapping as the scene (src/render.rs FEAT_*: FOG, HDR).
// Compile with shaders/build.sh.
#include "frame.glsl"

layout(constant_id = 0) const uint FEATURES = 0u;
const bool FOG = (FEATURES & 2u) != 0u;
const bool HDR = (FEATURES & 4u) != 0u;

// Must match `PostPush` in src/post.rs (mode = 1 for reversed Z).
layout(push_constant) uniform Push {
    Frame frame;
    uint src;
    uint src2;
    vec2 texel;
    uint mode;
    uint pad;
    vec4 params;
} pc;

layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 out_color;

uint pcg(uint v) {
    uint state = v * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

float hash_u(uint a, uint b) { return float(pcg(a + pcg(b)) >> 8) / 16777216.0; }

vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

// As `display` in shaders/mesh.frag.
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

void main() {
    Frame f = pc.frame;
    vec2 ndc = v_uv * 2.0 - 1.0;
    vec4 a = f.inv_view_proj * vec4(ndc, pc.mode == 1u ? 1.0 : 0.0, 1.0);
    vec4 b = f.inv_view_proj * vec4(ndc, 0.5, 1.0);
    vec3 dir = normalize(b.xyz / b.w - a.xyz / a.w);

    vec3 c;
    if (dir.y >= 0.0) {
        float t = 1.0 - pow(1.0 - dir.y, f.sky_params.x);
        c = mix(f.sky_horizon.rgb, f.sky_zenith.rgb, t);
    } else {
        c = mix(f.sky_horizon.rgb, f.sky_ground.rgb, smoothstep(0.0, 0.15, -dir.y));
    }
    float cos_a = clamp(dot(dir, f.sun_dir.xyz), -1.0, 1.0);
    float angle = acos(cos_a);
    float radius = f.sky_zenith.w;
    float edge = 0.0015; // radians: about a pixel at common fields of view
    float disc = radius > 0.0 ? 1.0 - smoothstep(radius - edge, radius + edge, angle) : 0.0;
    c += f.sun_color.rgb * (disc * f.sky_horizon.w + f.sky_ground.w * (exp(-angle * 14.0) + 0.15 * exp(-angle * 3.0)));
    if (FOG) {
        float t = fog_transmittance(f, dir, 0.0, true);
        c = mix(fog_radiance(f, dir), c, t);
    }
    out_color = vec4(display(c), 1.0);
}
