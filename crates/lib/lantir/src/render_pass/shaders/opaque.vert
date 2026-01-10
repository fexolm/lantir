#version 450

layout(push_constant) uniform Push {
    mat4 view;
    mat4 proj;
    mat4 viewproj;
} pc;

// Matches `crates/lib/lantir/src/resources/mod.rs`.
struct Vertex {
    vec3 position;
    vec3 normal;
    vec4 color;
    vec2 uv;
};

// Matches `DrawItem` layout but avoids int64 requirements by treating `u64` as two `u32`.
struct DrawItem {
    mat4 transform;
    uvec2 mesh;
    uvec2 material;
};

layout(set = 0, binding = 0, std430) readonly buffer VertexBuffer {
    Vertex vertices[];
} vb;

layout(set = 0, binding = 3, std430) readonly buffer DrawItemsBuffer {
    DrawItem items[];
} dib;

layout(location = 0) out vec4 vColor;
layout(location = 1) out vec2 vUv;
layout(location = 2) flat out uint vMaterialId;

void main() {
    // `OpaquePass` sets `first_instance = draw_item_index` for each indirect command.
    uint draw_id = gl_InstanceIndex;

    DrawItem item = dib.items[draw_id];
    Vertex v = vb.vertices[gl_VertexIndex];

    gl_Position = pc.viewproj * item.transform * vec4(v.position, 1.0);
    vColor = v.color;
    vUv = v.uv;
    vMaterialId = item.material.x;
}
