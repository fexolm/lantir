#include "common.hlsli"

[[vk::push_constant]] ConstantBuffer<DynamicConstants> pc;

[[vk::binding(2, 0)]] Texture2D textures[1024];
[[vk::binding(4, 0)]] SamplerState samplers[1024];
[[vk::binding(5, 0)]] ConstantBuffer<Skybox> skybox;

struct V2F
{
    float4 position : SV_Position;
    float2 ndc : TEXCOORD0;
};

[shader("vertex")]
V2F vs_main(uint vertexId : SV_VertexID)
{
    // Fullscreen triangle
    float2 p;
    if (vertexId == 0) p = float2(-1.0, -1.0);
    else if (vertexId == 1) p = float2(-1.0,  3.0);
    else p = float2( 3.0, -1.0);

    V2F o;
    o.position = float4(p, 1.0, 1.0);
    o.ndc = p;
    return o;
}

[shader("pixel")]
float4 ps_main(V2F input) : SV_Target0
{
    const uint INVALID = 0xFFFFFFFFu;
    if (skybox.tex == INVALID || skybox.sampler == INVALID)
    {
        return float4(0.0, 0.0, 0.0, 1.0);
    }

    // Reconstruct a world-space ray direction from NDC.
    float4 clip = float4(input.ndc, 1.0, 1.0);
    float4 world = mul(pc.inv_viewproj, clip);
    float3 world_pos = world.xyz / max(world.w, 1e-6);
    float3 dir = normalize(world_pos - pc.camera_pos);

    float2 uv = dir_to_equirect_uv(dir);

    int texIndex = clamp((int)skybox.tex, 0, 1023);
    int sampIndex = clamp((int)skybox.sampler, 0, 1023);

    float3 hdr = textures[texIndex].SampleLevel(samplers[sampIndex], uv, 0).rgb;
    hdr *= skybox.exposure;

    float3 ldr = tonemap_reinhard(hdr);
    ldr = linear_to_srgb(ldr);

    return float4(ldr, 1.0);
}
