// Shared by the overlay stages. Included; not compiled on its own.
#include "frame.glsl"

// One quad; must match `OverlayRecord` in src/overlay.rs.
struct Quad {
    vec4 rect;   // x, y, width, height (pixels, origin top-left)
    vec4 uv;     // u0, v0, u1, v1
    vec4 color;  // linear RGB, straight alpha
    vec4 params; // corner radius (pixels)
    uvec4 info;  // texture slot + 1 (0 = none), kind (1 = alpha mask)
};
layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer Quads {
    Quad items[];
};

// Must match `PushConstants` in src/render.rs (the quads take the draw records' place).
layout(push_constant) uniform Push {
    Frame frame;
    Quads quads;
    uint material;
    uint pass;
} pc;
