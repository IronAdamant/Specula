#version 460
// Vector shapes, fragment stage: the interpolated vertex colour (its alpha fades to 0 across the
// one-pixel fringe: anti-aliased edges), fogged, premultiplied. Compile with shaders/build.sh.
#include "sprite.glsl"

layout(constant_id = 0) const uint FEATURES = 0u;
const bool FOG = (FEATURES & 2u) != 0u;
const bool HDR = (FEATURES & 4u) != 0u;

layout(location = 0) in vec4 v_color;
layout(location = 1) in vec3 v_world;

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

// The mesh pipeline's `display`.
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
    float a = clamp(v_color.a, 0.0, 1.0);
    if (a <= 0.0) {
        discard;
    }
    vec3 c = v_color.rgb;
    if (FOG) {
        vec3 d = v_world - pc.frame.camera_time.xyz;
        float dist = length(d);
        vec3 dir = d / max(dist, 1e-6);
        float t = fog_transmittance(pc.frame, dir, dist, false);
        c = c * t + fog_radiance(pc.frame, dir) * (1.0 - t);
    }
    out_color = vec4(display(c) * a, a);
}
