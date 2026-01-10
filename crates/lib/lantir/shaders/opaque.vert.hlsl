struct Push
{
    column_major float4x4 view;
    column_major float4x4 proj;
    column_major float4x4 viewproj;
};

[[vk::push_constant]] ConstantBuffer<Push> pc;

// Matches `crates/lib/lantir/src/resources/mod.rs`.
struct Vertex
{
    float3 position;
    float3 normal;
    float4 color;
    float2 uv;
};

// Matches `DrawItem` layout but avoids int64 requirements by treating `u64` as two `u32`.
struct DrawItem
{
    column_major float4x4 transform;
    uint2 mesh;
    uint2 material;
};

[[vk::binding(0, 0)]] StructuredBuffer<Vertex> vb;
[[vk::binding(3, 0)]] StructuredBuffer<DrawItem> dib;

struct VSOut
{
    float4 position : SV_Position;
    float4 color : TEXCOORD0;
    float2 uv : TEXCOORD1;
    nointerpolation uint materialId : TEXCOORD2;
};

VSOut main(uint vertexId : SV_VertexID, uint instanceId : SV_InstanceID)
{
    // `OpaquePass` sets `first_instance = draw_item_index` for each indirect command.
    uint drawId = instanceId;

    DrawItem item = dib[drawId];
    Vertex v = vb[vertexId];

    VSOut o;
    o.position = mul(pc.viewproj, mul(item.transform, float4(v.position, 1.0)));
    o.color = v.color;
    o.uv = v.uv;
    o.materialId = item.material.x;
    return o;
}
