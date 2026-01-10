struct PSIn
{
    float4 color : TEXCOORD0;
    float2 uv : TEXCOORD1;
    nointerpolation uint materialId : TEXCOORD2;
};

// Matches `PbrMaterial` layout but avoids int64 requirements by treating `u64` as two `u32`.
struct PbrMaterial
{
    uint2 albedo_tex;
    uint2 albedo_sampler;
    uint2 normal_tex;
    uint2 normal_sampler;
    uint2 metallic_roughness_tex;
    uint2 metallic_roughness_sampler;
    uint2 emissive_tex;
    uint2 emissive_sampler;

    float4 base_color;
    float3 emissive_color;
    float metallness;
    float roughness;
};

[[vk::binding(1, 0)]] StructuredBuffer<PbrMaterial> materials;

[[vk::binding(2, 0)]] Texture2D textures[1024];
[[vk::binding(4, 0)]] SamplerState samplers[1024];

float4 main(PSIn input) : SV_Target0
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
    }

    return base;
}
