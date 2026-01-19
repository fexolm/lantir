#include "common.hlsli"

[[vk::push_constant]] ConstantBuffer<DynamicConstants> pc;

[[vk::binding(0, 0)]] StructuredBuffer<Vertex> vb;
[[vk::binding(1, 0)]] StructuredBuffer<PbrMaterial> materials;
[[vk::binding(2, 0)]] Texture2D textures[1024];
[[vk::binding(3, 0)]] StructuredBuffer<DrawItem> dib;
[[vk::binding(4, 0)]] SamplerState samplers[1024];
[[vk::binding(5, 0)]] ConstantBuffer<Skybox> skybox;

[[vk::constant_id(0)]] const uint PBR_ALPHA_TEST = 0u;

struct V2F
{
    float4 position : SV_Position;
    float4 color : TEXCOORD0;
    float2 uv : TEXCOORD1;
    nointerpolation uint material_id : TEXCOORD2;
    float3 normal : TEXCOORD3;
};

[shader("vertex")]
V2F vs_main(uint vertexId : SV_VertexID, uint instanceId : SV_InstanceID)
{
    // Passes set `first_instance = draw_item_index` for each indirect command.
    uint drawId = instanceId;

    DrawItem item = dib[drawId];
    Vertex v = vb[vertexId];

    V2F o;
    o.position = mul(pc.viewproj, mul(item.model_matrix, float4(v.position, 1.0)));
    o.color = v.color;
    o.uv = v.uv;
    o.material_id = item.material.x;
    o.normal = normalize(mul((float3x3)item.normal_matrix, v.normal));
    return o;
}

[shader("pixel")]
float4 ps_main(V2F input) : SV_Target0
{
    const uint INVALID = 0xFFFFFFFFu;

    float4 base = input.color;

    float alphaCutoff = 0.0;

    if (input.material_id != INVALID)
    {
        PbrMaterial mat = materials[input.material_id];
        base *= mat.base_color;
        alphaCutoff = mat.alpha_cutoff;

        uint albedoId = mat.albedo_tex.x;
        uint albedoSamplerId = mat.albedo_sampler.x;
        if (albedoId != INVALID && albedoSamplerId != INVALID)
        {
            int texIndex = clamp((int)albedoId, 0, 1023);
            int sampIndex = clamp((int)albedoSamplerId, 0, 1023);
            base *= textures[texIndex].Sample(samplers[sampIndex], input.uv);
        }
    }

    if (PBR_ALPHA_TEST != 0u)
    {
        if (base.a < alphaCutoff)
        {
            discard;
        }
    }

    // Simple ambient from skybox: sample along world normal.
    if (skybox.tex != INVALID && skybox.sampler != INVALID)
    {
        int texIndex = clamp((int)skybox.tex, 0, 1023);
        int sampIndex = clamp((int)skybox.sampler, 0, 1023);
        float3 hdr = textures[texIndex].SampleLevel(samplers[sampIndex], dir_to_equirect_uv(input.normal), 0).rgb;
        hdr *= skybox.exposure;
        float3 amb = saturate(tonemap_reinhard(hdr));
        base.rgb *= (skybox.ambient_floor + (1.0 - skybox.ambient_floor) * amb);
    }

    return base;
}
