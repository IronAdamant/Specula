#version 460
// Stylised water, fragment stage (src/water.rs, docs/design/TERRAIN.md): a surface on the draw's
// quad (mesh.vert), drawn after the opaque scene with no depth attachment. It reads the scene's
// depth: the floor under the pixel gives the water's thickness along the view ray and its depth
// straight down, hence the colour, the opacity (premultiplied over the scene behind: refraction
// without distortion) and shore foam. Waves are normals only (the quad stays flat, so the depth
// test and the ID buffer agree with the image), a function of the frame's time: captures are
// reproducible. Reflections: the sky and ambient with the sun's highlight, by Fresnel.
// Compile with shaders/build.sh.
#extension GL_EXT_nonuniform_qualifier : require
#define GI_FIELDS
#include "frame.glsl"

// Must match `PushConstants` in src/render.rs (src/water.rs `record_water`).
layout(push_constant) uniform Push {
    Frame frame;
    Draws draws;
    uint material; // bit 0: reversed Z
    uint pass;     // bindless slot of the scene's depth
    Bones bones;
} pc;

#include "lit_common.glsl"

layout(set = 0, binding = 1) uniform sampler2D textures[];

layout(location = 0) in vec3 v_world;
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec3 v_local;
layout(location = 3) flat in uint v_draw;

layout(location = 0) out vec4 out_color;

// Record fields (src/water.rs `fields`): color = deep + opacity, color2 = shallow + clarity,
// params = emissive, foam width, roughness, reflect; ext = wave scale, wave speed (f32 bits),
// foam RGB8 | pattern << 24, wave strength (unorm16).
#define DRAW pc.draws.items[v_draw]
const uint PATTERN_WAVES = 0u;
const uint PATTERN_ICE = 1u;
const uint PATTERN_LAVA = 2u;

// What the surface mirrors along `dir` (the caller dims it by the baked GI's occlusion). With the sky drawn,
// the sky's own gradient (the moonlit sky at night: the ambient fill is brighter than that sky,
// and mirrored it turned night water pale) plus the sun's or moon's glint; without, the ambient,
// as glossy meshes reflect.
vec3 water_reflection(vec3 p, vec3 dir) {
    if (pc.frame.sky_params.y > 0.5) {
        float t = clamp(dir.y, 0.0, 1.0);
        vec3 sky_c = mix(pc.frame.sky_horizon.rgb, pc.frame.sky_zenith.rgb, 1.0 - pow(1.0 - t, max(pc.frame.sky_params.x, 0.1)));
        float sun = max(dot(dir, pc.frame.sun_dir.xyz), 0.0);
        sun *= sun;
        sun *= sun;
        sun *= sun * sun;
        return min(sky_c, env_radiance(dir)) + pc.frame.sun_color.rgb * 4.0 * sun;
    }
    return env_radiance(dir);
}

// Worley distances (nearest, second nearest) in 2D: the ice's cracks.
vec2 worley(vec2 x) {
    ivec2 i = ivec2(floor(x));
    vec2 f = fract(x);
    float d1 = 8.0, d2 = 8.0;
    for (int y = -1; y <= 1; y++) {
        for (int k = -1; k <= 1; k++) {
            ivec2 c = i + ivec2(k, y);
            vec2 o = vec2(hash_i2(c), hash_i2(c + ivec2(57, 113)));
            vec2 r = vec2(k, y) + o - f;
            float d = dot(r, r);
            if (d < d1) {
                d2 = d1;
                d1 = d;
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    return sqrt(vec2(d1, d2));
}

// A float hash ("hash without sine", Dave Hoskins): cheaper than integer hashes on the iGPU; the
// same GPU repeats it exactly, so captures stay reproducible.
float whash(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Value noise and its gradient (quintic fade): x = 0..1, yz = d/dp.
vec3 vnoise_d(vec2 p) {
    vec2 i = floor(p);
    vec2 f = p - i;
    vec2 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    vec2 du = 30.0 * f * f * (f * (f - 2.0) + 1.0);
    float a = whash(i);
    float b = whash(i + vec2(1.0, 0.0));
    float c = whash(i + vec2(0.0, 1.0));
    float d = whash(i + vec2(1.0, 1.0));
    float k1 = b - a, k2 = c - a, k4 = a - b - c + d;
    return vec3(a + k1 * u.x + k2 * u.y + k4 * u.x * u.y, du * vec2(k1 + k4 * u.y, k2 + k4 * u.x));
}

// Slope (d height / d q) of the wave field at surface coordinates `q` (metres) and time `t`.
// `fp`: metres a pixel covers there; detail shorter than about 8 pixels fades out (it would alias
// into moire far away).
vec2 wave_slope(vec2 q, float t, float scale, float speed, float strength, float fp) {
    // Mostly noise: four octaves of value noise's gradient, each rotated (no axis lines), drifting
    // its own way (the surface evolves instead of sliding) and about twice the last's frequency
    // by an irrational step, so nothing repeats: no grid, no parallel bands at any distance.
    const mat2 ROT[4] = mat2[](mat2(0.966, 0.259, -0.259, 0.966), mat2(0.423, -0.906, 0.906, 0.423), mat2(-0.616, 0.788, -0.788, -0.616), mat2(0.139, 0.990, -0.990, 0.139));
    const vec2 DRIFT[4] = vec2[](vec2(0.71, 0.70), vec2(-0.83, 0.55), vec2(0.30, -0.95), vec2(-0.6, -0.8));
    vec2 s = vec2(0.0);
    float f = 1.0 / scale;
    float amp = 0.16 * strength;
    for (int i = 0; i < 4; i++) {
        float aa = clamp((1.0 / (f * max(fp, 1e-6)) - 4.0) / 8.0, 0.0, 1.0);
        if (aa > 0.0) {
            vec2 p = ROT[i] * (q * f) + DRIFT[i] * (t * speed * f * 0.6) + float(i) * 17.3;
            vec3 nd = vnoise_d(p);
            // d/dq of the octave's height (amp / f) * noise(R q f): amp * R^T * grad.
            s += (transpose(ROT[i]) * nd.yz) * (amp * aa);
        }
        f *= 2.13;
        amp *= 0.72;
    }
    // (No periodic swell on top: even a faint one reads as evenly spaced bands at a grazing
    // angle, sight-skeptic 2026-10-30.)
    return s;
}

void main() {
    mat4 model = DRAW.model;
    vec3 up = normalize(cross(model[2].xyz, model[0].xyz));
    vec3 tx = normalize(model[0].xyz);
    vec3 tz = normalize(cross(tx, up));
    // Surface coordinates and the metres a pixel covers there (derivatives before any branch).
    vec2 q = vec2(dot(v_world, tx), dot(v_world, tz));
    vec2 dq = fwidth(q);
    float fp = max(dq.x, dq.y);
    // The opaque scene under this pixel, and the floor there: its world position from the scene
    // depth (none: the sky, far away). Worked out before the depth test's discard, so the depth's
    // screen derivative is defined (the waterline's anti-aliasing).
    float zs = texelFetch(textures[nonuniformEXT(pc.pass)], ivec2(gl_FragCoord.xy), 0).r;
    bool reverse_z = (pc.material & 1u) != 0u;
    bool sky = reverse_z ? zs <= 0.0 : zs >= 1.0;
    vec2 ndc = gl_FragCoord.xy / pc.frame.viewport.xy * 2.0 - 1.0;
    vec4 hp = pc.frame.inv_view_proj * vec4(ndc, sky ? 0.5 : zs, 1.0);
    vec3 floor_p = hp.xyz / hp.w;
    float floor_depth = dot(v_world - floor_p, up);
    // About how much the depth changes from one pixel to the next (capped: at the sky's edge the
    // floor jumps).
    float depth_px = min(fwidth(floor_depth), 2.0);
    // The same for the waterline's fade, but not across a jump in the floor (a crest's silhouette
    // over the floor behind it, half a metre or more from one pixel to the next): widening the
    // fade there opened the surface in 2 x 2 blocks along every submerged crest, a dotted line.
    float fade_px = depth_px < 0.5 ? depth_px : 0.0;
    // Nearer than the water: hidden (the shader's depth test), except a floor barely out of the
    // water (under `rise`, 12 cm): the waterline fades over it instead, so a crest grazing the
    // surface doesn't break it into a dotted line of triangle tips. Never more, at any distance:
    // the floor may be any mesh (a post standing in the water), and water must never hide it.
    const float rise = 0.12;
    if ((reverse_z ? zs > gl_FragCoord.z : zs < gl_FragCoord.z) && floor_depth < -rise) {
        discard;
    }
    vec3 cam = camera_pos();
    vec3 to_eye = cam - v_world;
    float eye_dist = length(to_eye);
    vec3 v = to_eye / max(eye_dist, 1e-6);
    bool below = dot(to_eye, up) < 0.0;
    float thickness = sky ? 1e4 : length(floor_p - v_world);
    float depth = sky ? 1e4 : max(floor_depth, 0.0);

    vec4 c0 = DRAW.color;
    vec4 c2 = DRAW.color2;
    vec4 pr = DRAW.params;
    uvec4 ext = DRAW.ext;
    float scale = uintBitsToFloat(ext.x);
    float speed = uintBitsToFloat(ext.y);
    uint pattern = ext.z >> 24;
    vec3 foam_col = vec3(ext.z & 255u, (ext.z >> 8) & 255u, (ext.z >> 16) & 255u) / 255.0;
    float strength = float(ext.w & 65535u) / 65535.0;
    float clarity = max(c2.a, 0.01);
    float t = pc.frame.camera_time.w;
    // Normal.
    vec2 slope = vec2(0.0);
    float crack = 0.0;
    float crust = 0.0;
    if (pattern == PATTERN_ICE) {
        float cell = max(scale * 0.35, 0.05);
        vec2 w = worley(q / cell);
        crack = 1.0 - smoothstep(0.0, 0.035, w.y - w.x);
        // Cracks thinner than a few pixels fade out (they sparkle into noise far away).
        crack *= clamp((cell / max(fp, 1e-6) - 6.0) / 10.0, 0.0, 1.0);
        slope = vec2(value_noise2(q * 0.7) - 0.5, value_noise2(q * 0.7 + 31.0) - 0.5) * 0.06 * strength;
    } else if (pattern == PATTERN_LAVA) {
        vec2 flow = q / scale + vec2(t * speed * 0.13, t * speed * 0.07);
        float g = value_noise2(flow) * 0.6 + value_noise2(flow * 2.3 + 5.0) * 0.3 + value_noise2(flow * 5.1 + 9.0) * 0.1;
        // Dark crust everywhere but thin glowing veins along the noise's mid line (and the odd
        // hot pool where it lingers there).
        float vein = abs(g - 0.5);
        crust = smoothstep(0.02, 0.07, vein);
        slope = (vec2(value_noise2(flow * 3.0 + 1.0), value_noise2(flow * 3.0 + 7.0)) - 0.5) * 0.3 * strength * crust;
    } else {
        slope = wave_slope(q, t, max(scale, 0.05), speed, strength, fp);
    }
    vec3 n = normalize(up - tx * slope.x - tz * slope.y);
    if (below) {
        n = -n;
    }

    float rough = clamp(pr.z, 0.02, 1.0);
    float reflect_k = clamp(pr.w, 0.0, 1.0);
    vec3 deep = c0.rgb;
    vec3 shallow = c2.rgb;
    float ndv = clamp(dot(n, v), 0.0, 1.0);
    float mm = 1.0 - ndv;
    float m2 = mm * mm;
    float fresnel = (0.02 + 0.98 * (m2 * m2 * mm)) * reflect_k;
    vec3 diffuse, specular;
    float sun;
    // The baked GI's ambient (once: the body, foam and depths reuse it): per pixel at tier high,
    // the water draw's ambient cube at low (src/render.rs samples water draws so whatever their
    // size: per pixel, the lean probes cost the iGPU 2.2 ms of a pond filling the view).
    vec3 amb = GI_PIXEL ? gi_ambient(v_world, n, v) : gi_cube_draw(v_draw, n);
    gather_light(v_world, n, v, rough, amb, diffuse, specular, sun);

    // Body: colour by depth, opacity by thickness; lit by what reaches the surface.
    float mix_k = 1.0 - exp(-depth / clarity);
    vec3 body = mix(shallow, deep, mix_k);
    // At a grazing angle the light that comes back has scattered near the surface: lean to the
    // shallow colour, so a preset's own colour carries into the distance (not a dark charcoal).
    body = mix(body, shallow, (1.0 - ndv) * 0.8);
    float body_a = clamp(c0.a * (1.0 - exp(-thickness / clarity)), 0.0, 1.0);
    // Shallow water still takes its own colour quickly: through clear water a bright floor right
    // under the surface (a bank, a submerged crest) showed as a pale line at a distance.
    body_a = max(body_a, c0.a * 0.45 * smoothstep(0.0, 0.5, depth));
    vec3 body_light = body * (0.35 * diffuse + 0.15 * amb);
    // Shore foam: soft, broken patches in shallow water, never an outline. It fades out with
    // distance: past the terrain's fine rings the floor under the water is coarse triangles, and
    // foam would trace them (straight runs, plates over low ground); the shallow colour and the
    // opacity by thickness carry the shore there.
    float foam_w = pr.y;
    float foam = 0.0;
    float near = 1.0 - smoothstep(15.0, 40.0, eye_dist);
    if (foam_w > 0.0 && !sky && near > 0.0) {
        float edge = 1.0 - smoothstep(0.0, foam_w, depth);
        if (edge > 0.0) {
            // Only near the shore: the breakup noise costs.
            float breakup = value_noise2(q * 1.7 + vec2(t * speed * 0.2, 0.0)) * 0.65 + value_noise2(q * 4.3 - vec2(0.0, t * speed * 0.3)) * 0.35;
            // Patches where the noise beats the depth: clumps along the waterline with gaps
            // between them, specks further out; never a continuous outline.
            // (Clumps about a metre and a half across: on a steep bank the band is a few pixels
            // wide, and longer clumps would read as a line.)
            float clump = value_noise2(q * 0.7 + vec2(3.1, t * speed * 0.05));
            foam = smoothstep(0.5, 0.85, edge * breakup * 1.5) * smoothstep(0.55, 0.72, clump) * 0.65 * near;
            // Not on the waterline itself (the water is still fading in there), and not where the
            // foam band is only a few pixels wide (a steep bank): there it can only be a line.
            foam *= smoothstep(0.03, 0.15, depth) * smoothstep(4.0, 10.0, foam_w / max(depth_px, 1e-4));
        }
    }
    vec3 c;
    float alpha;
    if (pattern == PATTERN_LAVA) {
        // Opaque and glowing: bright veins in a dark crust. Towards the shore a band of cooled,
        // solid crust (no veins, at any distance), and the edge itself fades over its first
        // 30 cm of depth: no hard, angular rim against the terrain's triangles.
        float glow = pr.x;
        vec3 hot = shallow * glow * (0.6 + 0.4 * value_noise2(q * 0.5 + t * 0.05));
        vec3 crust_c = deep * diffuse;
        float band = sky ? 0.0 : 1.0 - smoothstep(0.0, max(foam_w, 0.3) * 2.5, depth + (value_noise2(q * 0.4) - 0.5) * foam_w);
        crust = max(crust, band);
        c = mix(hot, crust_c, crust);
        c = mix(c, foam_col * diffuse, max(foam, band * 0.6));
        c += specular * 0.15 * crust;
        alpha = sky ? 1.0 : smoothstep(-rise, 0.3, floor_depth);
        c *= alpha;
    } else {
        // The reflection takes some of the water's own colour, so a preset still reads at the
        // horizon (where Fresnel makes every water a mirror of the same sky).
        vec3 tint = (deep + shallow) / max(max(deep.r + shallow.r, max(deep.g + shallow.g, deep.b + shallow.b)), 1e-3);
        // Baked GI dims the mirrored sky where the surroundings are dark (a pool in a cave).
        vec3 refl = water_reflection(v_world, reflect(-v, n)) * gi_occlusion(amb) * mix(vec3(1.0), tint, 0.6) + specular * (1.0 - rough);
        if (pattern == PATTERN_ICE) {
            // Ice has body: frosted and nearly opaque where it is thick, thinning to clear at the
            // shore. Cracks are part of that body (paler, never more opaque than the ice), and
            // fade where it is thin, so they never draw a white net over the ground beneath.
            float thick = sky ? 1.0 : smoothstep(0.0, 1.2, depth);
            body_a = max(body_a, mix(0.25, 0.92, thick) * smoothstep(0.0, 0.15, depth));
            body_light = mix(body_light, foam_col * (0.5 * diffuse + 0.3 * amb), crack * 0.22 * thick);
        }
        c = body_light * body_a * (1.0 - fresnel) + refl * fresnel;
        alpha = body_a + fresnel * (1.0 - body_a);
        if (foam > 0.0) {
            vec3 f = foam_col * (0.5 * diffuse + 0.5 * amb);
            c = mix(c, f, foam);
            alpha = mix(alpha, 1.0, foam);
        }
        // The waterline: anti-aliased over about two pixels of the depth's change (and at least
        // 5 cm), not a band of fixed depth. A wide fade (60 cm) left holes in the surface's
        // reflection over every submerged crest, which read as pale lines at a distance; with
        // none, the line was a hard step along the floor's triangles.
        float soft = sky ? 1.0 : smoothstep(-rise, max(fade_px * 2.0, 0.05), floor_depth);
        c *= soft;
        alpha *= soft;
        if (below) {
            // From below: a plain tinted ceiling.
            c = deep * amb * c0.a;
            alpha = c0.a;
        }
    }
    if (FOG) {
        float tr;
        c = fogged_at(v_world, c, alpha, tr);
    }
    // SSAO: blended layers keep what they leave visible of the ambient below (the blend), and add
    // none of their own.
    out_color = vec4(display(c), alpha);
}
