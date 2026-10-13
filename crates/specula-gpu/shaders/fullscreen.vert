#version 460
// One triangle covering the viewport; v_uv is 0..1 across it (origin top-left).
// Compile with shaders/build.sh.

layout(location = 0) out vec2 v_uv;

void main() {
    vec2 p = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
