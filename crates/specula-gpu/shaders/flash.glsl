// The flash watch's per-texel state (crates/specula/src/flashwatch.rs mirrors it on the CPU for
// captures; keep the two the same). Included by flash_state.frag and flash_cells.frag.
//
// State, RGBA16F at half resolution (each texel a 2x2 block of the final image):
//   r: relative luminance at the last extreme; b: red value / 320 at the last extreme;
//   g: 256 x the luminance code + the texel's mean luminance this frame (0-255);
//   a: 256 x the red code + its mean red value / 320 this frame (0-255).
//   (Whole numbers up to 2047 are exact in half floats.)
//   Luminance code: its direction (0 none, 1 down, 2 up), + 3 when it made a transition this frame.
//   Red code: its direction (0 not started, 1 none, 2 down, 3 up), + 4 when it made a transition.
// A transition: luminance moving 0.1 back from an extreme with the darker side under 0.8, or the
// red value ((R - G - B) x 320 while saturated red, R / (R + G + B) >= 0.8) moving more than 20.
// The blocks read only this state (each texel's means and transitions), never the final image.

// (code, mean 0-255) of a packed channel.
ivec2 unpack_state(float v) { int i = int(v + 0.5); return ivec2(i >> 8, i & 255); }
float pack_state(int code, float mean) { return float(code * 256) + round(clamp(mean, 0.0, 1.0) * 255.0); }

float lum_dir(int code) { int d = code % 3; return d == 0 ? 0.0 : (d == 1 ? -1.0 : 1.0); }
int lum_code(float d, bool moved) { return (d == 0.0 ? 0 : (d < 0.0 ? 1 : 2)) + (moved ? 3 : 0); }
bool lum_moved(int code) { return code >= 3; }
float red_dir(int code) { int d = code & 3; return d < 2 ? 0.0 : (d == 2 ? -1.0 : 1.0); }
int red_code(float d, bool moved) { return (d == 0.0 ? 1 : (d < 0.0 ? 2 : 3)) + (moved ? 4 : 0); }
bool red_started(int code) { return (code & 3) != 0; }
bool red_moved(int code) { return code >= 4; }

bool counts_lum(float a, float b) { return abs(a - b) >= 0.1 && min(a, b) < 0.8; }
bool counts_red(float a, float b) { return abs(a - b) > 20.0 / 320.0; }

// Moves the extreme `ext` (direction `dir`) on to value `v`.
void step_lum(inout float ext, inout float dir, float v) {
    if (dir == 0.0) {
        if (counts_lum(ext, v)) { dir = v > ext ? 1.0 : -1.0; ext = v; }
    } else if (dir > 0.0) {
        if (v > ext) { ext = v; } else if (counts_lum(ext, v)) { dir = -1.0; ext = v; }
    } else {
        if (v < ext) { ext = v; } else if (counts_lum(ext, v)) { dir = 1.0; ext = v; }
    }
}

void step_red(inout float ext, inout float dir, float v) {
    if (dir == 0.0) {
        if (counts_red(ext, v)) { dir = v > ext ? 1.0 : -1.0; ext = v; }
    } else if (dir > 0.0) {
        if (v > ext) { ext = v; } else if (counts_red(ext, v)) { dir = -1.0; ext = v; }
    } else {
        if (v < ext) { ext = v; } else if (counts_red(ext, v)) { dir = 1.0; ext = v; }
    }
}

// (relative luminance, red value / 320) of a linear colour.
vec2 measure(vec3 c) {
    float lum = dot(c, vec3(0.2126, 0.7152, 0.0722));
    float s = c.r + c.g + c.b;
    float red = (s > 0.0 && c.r / s >= 0.8) ? max(c.r - c.g - c.b, 0.0) : 0.0;
    return vec2(lum, red);
}
