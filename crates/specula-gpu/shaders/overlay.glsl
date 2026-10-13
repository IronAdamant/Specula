// Shared by the overlay stages. Included; not compiled on its own.
#include "frame.glsl"

// One quad or triangle; must match `OverlayRecord` in src/overlay.rs. Colours are linear RGB,
// straight alpha.
struct Quad {
    vec4 rect;     // quad: x, y, width, height (pixels, origin top-left) | triangle: p0.xy, p1.xy
    vec4 uv;       // quad: u0, v0, u1, v1                                 | triangle: p2.xy
    vec4 color;    // quad: fill at the top                                | triangle: colour of p0
    vec4 color2;   // quad: fill at the bottom                             | triangle: colour of p1
    vec4 border0;  // quad: border at the top                              | triangle: colour of p2
    vec4 border1;  // quad: border at the bottom
    vec4 params;   // corner radius, border width, feather (>= 1), unused
    vec4 clip;     // x0, y0, x1, y1 (whole pixels)
    uvec4 info;    // texture slot + 1 (0 = none), kind (0 rgba, 1 alpha mask, 2 triangle)
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Quads {
    Quad items[];
};

const uint KIND_ALPHA_MASK = 1u;
const uint KIND_TRIANGLE = 2u;

// Must match `PushConstants` in src/render.rs (the quads take the draw records' place).
layout(push_constant) uniform Push {
    Frame frame;
    Quads quads;
    uint material;
    uint pass;
} pc;
