#include "common.hlsli"

struct Push
{
    column_major float4x4 view;
    column_major float4x4 proj;
    column_major float4x4 viewproj;
};

[[vk::push_constant]] ConstantBuffer<Push> pc;

[[vk::binding(0, 0)]] StructuredBuffer<Vertex> vb;
[[vk::binding(1, 0)]] StructuredBuffer<PbrMaterial> materials;
[[vk::binding(2, 0)]] Texture2D textures[1024];
[[vk::binding(3, 0)]] StructuredBuffer<DrawItem> dib;
[[vk::binding(4, 0)]] SamplerState samplers[1024];

struct V2F
{
    float4 position : SV_Position;
    float4 color : TEXCOORD0;
    float2 uv : TEXCOORD1;
    nointerpolation uint materialId : TEXCOORD2;
};

[shader("vertex")]
V2F vs_main(uint vertexId : SV_VertexID, uint instanceId : SV_InstanceID)
{
    // `OpaquePass` sets `first_instance = draw_item_index` for each indirect command.
    uint drawId = instanceId;

    DrawItem item = dib[drawId];
    Vertex v = vb[vertexId];

    V2F o;
    o.position = mul(pc.viewproj, mul(item.transform, float4(v.position, 1.0)));
    o.color = v.color;
    o.uv = v.uv;
    o.materialId = item.material.x;
    return o;
}

[shader("pixel")]
float4 ps_main(V2F input) : SV_Target0
{
    const uint INVALID = 0xFFFFFFFFu;

    float4 base = input.color;

    if (input.materialId != INVALID)
    {
        PbrMaterial mat = materials[input.materialId];
        base *= mat.base_color;

        uint albedoId = mat.albedo_tex.x;
        uint albedoSamplerId = mat.albedo_sampler.x;
        if (albedoId != INVALID && albedoSamplerId != INVALID)
        {
            int texIndex = clamp((int)albedoId, 0, 1023);
            int sampIndex = clamp((int)albedoSamplerId, 0, 1023);
            base *= textures[texIndex].Sample(samplers[sampIndex], input.uv);
        }

        if(base.a < mat.alpha_cutoff) {
            discard;
        }

    }

    return base;
}
