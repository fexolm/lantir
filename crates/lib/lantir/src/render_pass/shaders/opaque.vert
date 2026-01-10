#version 450

layout(push_constant) uniform Push {
    mat4 render_matrix;
} pc;

void main() {
    // Minimal stub pipeline: full-screen-ish triangle without vertex buffers.
    // (Real mesh drawing will be added in stage B once draw submission is wired.)
    vec2 pos[3] = vec2[3](
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0)
    );
    gl_Position = vec4(pos[gl_VertexIndex], 0.0, 1.0);
}
