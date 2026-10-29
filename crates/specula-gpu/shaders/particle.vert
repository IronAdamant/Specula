#version 460
// Particles, vertex stage (src/effects.rs): instance i is particle i, six vertices a quad facing
// the camera (rotated, or stretched along a world vector for sparks). Everything per particle
// is worked out here, once per corner: the flipbook cell, lighting at the particle's centre
// (lit materials: ambient, sun and point lights, no normal: volumes), fog. The fragment stage
// gets flat varyings only (no record reads per fragment).
// Compile with shaders/build.sh.
#define GI_FIELDS
#include "particle.glsl"
#define NO_DISPLAY
#include "effects_light.glsl"
#include "gi.glsl"

layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec3 v_world;
layout(location = 2) flat out vec4 v_color;  // linear, lit and fogged; straight alpha
layout(location = 3) flat out uint v_slot;   // texture slot + 1
layout(location = 4) flat out vec2 v_params; // soft (m), additive

const vec2 QUAD[6] = vec2[](vec2(-0.5, -0.5), vec2(0.5, -0.5), vec2(0.5, 0.5), vec2(-0.5, -0.5), vec2(0.5, 0.5), vec2(-0.5, 0.5));

void main() {
    ParticleRec r = pc.particles.items[gl_InstanceIndex];
    MaterialRec m = pc.materials.items[r.b.z & 0xffffu];
    uint frame = r.b.z >> 16u;
    vec3 pos = r.a.xyz;
    float size = r.a.w;
    vec4 color = vec4(unpackHalf2x16(r.b.x), unpackHalf2x16(r.b.y));
    float rot = uintBitsToFloat(r.b.w);
    vec2 q = QUAD[gl_VertexIndex];
    vec3 fwd = ortho_view() ? pc.cam_forward.xyz : normalize(pos - pc.cam_pos.xyz);
    // The stretch as seen on screen (its part along the view ray doesn't show).
    vec3 s = r.c.xyz - fwd * dot(r.c.xyz, fwd);
    float sl = length(s);
    vec3 right;
    vec3 up;
    float w = size;
    if (sl > 1e-4) {
        right = s / sl;
        up = normalize(cross(fwd, right));
        w = size + sl;
    } else {
        float c = cos(rot);
        float sn = sin(rot);
        right = pc.cam_right.xyz * c + pc.cam_up.xyz * sn;
        up = pc.cam_up.xyz * c - pc.cam_right.xyz * sn;
    }
    vec3 world = pos + right * (q.x * w) + up * (q.y * size);
    gl_Position = pc.frame.view_proj * vec4(world, 1.0);
    v_world = world;
    uint cols = max(m.info.y, 1u);
    uint rows = max(m.info.z, 1u);
    uint f = frame % (cols * rows);
    vec2 cell = vec2(float(f % cols), float(f / cols));
    // Texture rows run top to bottom: the quad's top (q.y = 0.5) shows the cell's top.
    v_uv = (cell + vec2(q.x + 0.5, 0.5 - q.y)) / vec2(float(cols), float(rows));
    vec3 c = color.rgb;
    if ((m.info.w & MAT_LIT) != 0u) {
        c *= gi_ambient(pos, vec3(0.0), vec3(0.0)) + pc.frame.sun_color.rgb * 0.55 + point_lights(pos, vec3(0.0));
    }
    float additive = clamp(r.c.w, 0.0, 1.0);
    if (FOG) {
        vec3 d = pos - pc.frame.camera_time.xyz;
        float dist = length(d);
        vec3 dir = d / max(dist, 1e-6);
        float t = fog_transmittance(pc.frame, dir, dist, false);
        // Added light fades into the fog; covering (alpha) particles also take on its colour.
        c = c * t + fog_radiance(pc.frame, dir) * ((1.0 - t) * (1.0 - additive));
    }
    v_color = vec4(c, clamp(color.a, 0.0, 1.0));
    v_slot = m.info.x;
    v_params = vec2(m.params.x, additive);
}
