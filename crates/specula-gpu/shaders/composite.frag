#version 460
// Tone mapping: the exposed HDR scene plus bloom (src2, scaled by pc.bloom) -> display values,
// with the same ACES curve and dither as the forward path (shaders/mesh.frag `display`), and
// colour grading around it (src/lib.rs GradingSettings; src/post.rs). Each grading step runs only
// when its bit is set in pc.steps, so neutral steps change nothing.
// Compile with shaders/build.sh.
#define CUSTOM_PUSH
#include "post.glsl"

// Must match `TonemapPush` in src/post.rs.
layout(push_constant) uniform Push {
    Frame frame;
    uint src;       // the HDR scene (alpha: SSAO's ambient fraction)
    uint src2;      // the bloom blur (0xffffffff = none)
    vec2 texel;     // 1 / the frame's size
    uint steps;     // GRADE_* bits, the LUT's size in bits 16..24
    uint lut;       // the LUT's slot
    uint ao;        // SSAO visibility (0xffffffff = none)
    float bloom;    // bloom intensity
    uint pad0;
    uint pad1;
    vec4 balance;   // white balance LMS gains, w contrast
    vec4 lift;      // lift per channel, w saturation
    vec4 gamma;     // 1 / gamma per channel, w vibrance
    vec4 gain;      // gain per channel, w LUT strength
    vec4 fx;        // vignette, vignette radius, grain, unused
} pc;

const uint GRADE_BALANCE = 1u;
const uint GRADE_CONTRAST = 2u;
const uint GRADE_SATURATION = 4u;
const uint GRADE_LGG = 8u;
const uint GRADE_LUT = 16u;
const uint GRADE_VIGNETTE = 32u;
const uint GRADE_GRAIN = 64u;
const uint GRADE_LUT_SRGB = 128u;
const uint GRADE_GAMMA = 256u;
const uint GRADE_DISPLAY = GRADE_SATURATION | GRADE_LGG | GRADE_LUT | GRADE_VIGNETTE | GRADE_GRAIN;

uint pcg(uint v) {
    uint state = v * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

float hash_u(uint a, uint b) { return float(pcg(a + pcg(b)) >> 8) / 16777216.0; }

vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

float luma(vec3 c) { return dot(c, vec3(0.2126, 0.7152, 0.0722)); }

vec3 srgb_encode(vec3 c) {
    c = clamp(c, 0.0, 1.0);
    return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, greaterThan(c, vec3(0.0031308)));
}

vec3 srgb_decode(vec3 e) {
    e = clamp(e, 0.0, 1.0);
    return mix(e / 12.92, pow((e + 0.055) / 1.055, vec3(2.4)), greaterThan(e, vec3(0.04045)));
}

// White balance in LMS cone space (the matrices of Unity's post stack: linear Rec. 709 <-> LMS).
vec3 white_balance(vec3 c, vec3 gains) {
    vec3 lms = vec3(dot(vec3(3.90405e-1, 5.49941e-1, 8.92632e-3), c),
                    dot(vec3(7.08416e-2, 9.63172e-1, 1.35775e-3), c),
                    dot(vec3(2.31082e-2, 1.28021e-1, 9.36245e-1), c)) * gains;
    return vec3(dot(vec3(2.85847e+0, -1.62879e+0, -2.48910e-2), lms),
                dot(vec3(-2.10182e-1, 1.15820e+0, 3.24281e-4), lms),
                dot(vec3(-4.18120e-2, -1.18169e-1, 1.06867e+0), lms));
}

// A 3D LUT strip (N*N x N: N slices side by side, red across each slice, green down the rows,
// blue from slice to slice) applied to display values `e` (sRGB-encoded, 0..1).
vec3 lut(vec3 e, uint slot, float n, bool srgb) {
    e = clamp(e, 0.0, 1.0);
    float b = e.b * (n - 1.0);
    float b0 = floor(b);
    float b1 = min(b0 + 1.0, n - 1.0);
    vec2 rg = (e.rg * (n - 1.0) + 0.5) / vec2(n * n, n);
    vec3 lo = textureLod(textures[slot], vec2(rg.x + b0 / n, rg.y), 0.0).rgb;
    vec3 hi = textureLod(textures[slot], vec2(rg.x + b1 / n, rg.y), 0.0).rgb;
    vec3 c = mix(lo, hi, b - b0);
    return srgb ? srgb_encode(c) : c;
}

void main() {
    uint steps = pc.steps & 0xffffu;
    vec4 scene = texture(textures[pc.src], v_uv);
    vec3 c = scene.rgb;
    if (pc.ao != 0xffffffffu) {
        // SSAO: alpha is the luminance of this pixel's ambient light (src/ssao.rs); the occlusion
        // takes that part, as a fraction of the pixel's luminance. The occlusion (blurred
        // depth-aware at its own resolution) is upsampled bilinearly: a depth-aware upsample here
        // would read the full-resolution depth (+0.35 ms on the Intel iGPU at 1600x900) to
        // sharpen an edge one occlusion texel wide.
        float visibility = textureLod(textures[pc.ao], v_uv, 0.0).r;
        float y = luma(c);
        float ambient = y > 1e-6 ? clamp(scene.a / y, 0.0, 1.0) : 0.0;
        c *= 1.0 - (1.0 - visibility) * ambient;
    }
    if (pc.src2 != 0xffffffffu) {
        c += texture(textures[pc.src2], v_uv).rgb * pc.bloom;
    }
    // Scene-referred grading (exposed linear light).
    if ((steps & GRADE_BALANCE) != 0u) {
        c = max(white_balance(c, pc.balance.xyz), vec3(0.0));
    }
    if ((steps & GRADE_CONTRAST) != 0u) {
        // Log-space contrast of the luminance around mid grey (log2(y / 0.18) scales by
        // `contrast`), colours scaled along: hue and saturation stay, two transcendentals.
        float y = max(luma(c), 1e-10);
        c *= exp2(pc.balance.w * log2(y / 0.18)) * 0.18 / y;
    }
    float amount = 1.0; // saturation and vibrance, applied on display values below
    if ((steps & GRADE_SATURATION) != 0u) {
        // How saturated the light is, as the eye sees it (on roughly gamma-encoded scene light:
        // the tone curve would make a bright rune and dim stone look alike). Vibrance spares
        // saturated colours; applied after tone mapping, where a bright colour's dim channel isn't
        // amplified by the curve.
        vec3 p = sqrt(max(c, vec3(0.0)));
        float hi = max(p.r, max(p.g, p.b));
        float sat = (hi - min(p.r, min(p.g, p.b))) / max(hi, 1e-6);
        float muted = (1.0 - sat) * (1.0 - sat) * (1.0 - sat);
        amount = pc.lift.w * max(1.0 + pc.gamma.w * muted, 0.0);
    }
    bool tonemap = pc.frame.sky.w > 0.5;
    if (tonemap) {
        c = aces(c);
    }
    // Display-referred grading. Saturation, lift/gamma/gain and grain work on gamma-2 encoded values
    // (sqrt: close to the display's encoding, and cheap); the LUT on exact sRGB values, the
    // encoding LUT strips are authored in; the vignette scales linear light by its square.
    if ((steps & GRADE_DISPLAY) != 0u) {
        c = clamp(c, 0.0, 1.0);
        if ((steps & (GRADE_SATURATION | GRADE_LGG)) != 0u) {
            vec3 e = sqrt(c);
            if ((steps & GRADE_SATURATION) != 0u) {
                float y = luma(e);
                e = max(vec3(y) + (e - vec3(y)) * amount, vec3(0.0));
            }
            if ((steps & GRADE_LGG) != 0u) {
                e = max(pc.gain.xyz * (e + pc.lift.xyz * (1.0 - e)), vec3(0.0));
                if ((steps & GRADE_GAMMA) != 0u) {
                    e = pow(e, pc.gamma.xyz);
                }
            }
            c = e * e;
        }
        if ((steps & GRADE_LUT) != 0u) {
            vec3 e = srgb_encode(c);
            vec3 l = lut(e, pc.lut, float(pc.steps >> 16), (steps & GRADE_LUT_SRGB) != 0u);
            c = srgb_decode(mix(e, l, pc.gain.w));
        }
        if ((steps & GRADE_VIGNETTE) != 0u) {
            float aspect = pc.texel.y / pc.texel.x;
            vec2 d = (v_uv - 0.5) * vec2(aspect, 1.0);
            float r = length(d) / length(vec2(aspect, 1.0) * 0.5); // 1 at the corners
            float start = pc.fx.y;
            float t = clamp((r - start) / (1.0 - start), 0.0, 1.0);
            float v = 1.0 - pc.fx.x * t * t * (3.0 - 2.0 * t);
            c *= v * v;
        }
        if ((steps & GRADE_GRAIN) != 0u) {
            uint seed = pcg(floatBitsToUint(pc.frame.camera_time.w));
            float n = hash_u(uint(gl_FragCoord.x) ^ seed, uint(gl_FragCoord.y) + seed) - 0.5;
            vec3 e = sqrt(c);
            float y = clamp(luma(e), 0.0, 1.0);
            e = max(e + n * pc.fx.z * 0.1 * (0.3 + 2.8 * y * (1.0 - y)), vec3(0.0));
            c = e * e;
        }
    }
    if (tonemap) {
        float n = hash_u(uint(gl_FragCoord.x), uint(gl_FragCoord.y)) - 0.5;
        c += n * 2.2 * pow(max(c, vec3(1e-4)), vec3(0.545)) / 255.0;
    }
    out_color = vec4(max(c, vec3(0.0)), 1.0);
}
