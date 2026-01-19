#include "common.hlsli"

[[vk::push_constant]] ConstantBuffer<DynamicConstants> pc;

[[vk::binding(5, 0)]] ConstantBuffer<Skybox> skybox;

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
    float3 normalWs : TEXCOORD3;
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
    o.normalWs = normalize(mul((float3x3)item.transform, v.normal));
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
            base *= textures[albedoId].Sample(samplers[albedoSamplerId], input.uv);
        }
    }

    // Simple ambient from HDRI (diffuse-ish): sample along world normal.
    if (skybox.tex != INVALID && skybox.sampler != INVALID)
    {
        int texIndex = clamp((int)skybox.tex, 0, 1023);
        int sampIndex = clamp((int)skybox.sampler, 0, 1023);
        float3 hdr = textures[texIndex].SampleLevel(samplers[sampIndex], dir_to_equirect_uv(input.normalWs), 0).rgb;
        hdr *= skybox.exposure;
        float3 amb = saturate(tonemap_reinhard(hdr));
        base.rgb *= (skybox.ambient_floor + (1.0 - skybox.ambient_floor) * amb);
    }

    return base;
}
