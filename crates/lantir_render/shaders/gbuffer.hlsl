// gbuffer.hlsl — GBuffer geometry pass.
// Outputs: normal (RGBA16F), albedo (RGBA8), roughness+metallic (RG8).
// Depth is written via standard depth test.

#include "common.hlsli"

[[vk::push_constant]] ConstantBuffer<DynamicConstants> pc;

[[vk::binding(0, 0)]] StructuredBuffer<Vertex>       vertices;
[[vk::binding(1, 0)]] StructuredBuffer<PbrMaterial>  materials;
[[vk::binding(2, 0)]] Texture2D                      textures[1024];
[[vk::binding(3, 0)]] StructuredBuffer<DrawItem>     draw_items;
[[vk::binding(4, 0)]] SamplerState                   samplers[1024];

[[vk::constant_id(0)]] const uint GBUF_ALPHA_TEST = 0u;

struct V2F
{
    float4 position      : SV_Position;
    float2 uv            : TEXCOORD0;
    nointerpolation uint material_id : TEXCOORD1;
    float3 world_normal  : TEXCOORD2;
    float4 vert_color    : TEXCOORD3;
    float3 world_pos     : TEXCOORD4;
    // Tangent in world space. xyz = tangent direction, w = bitangent sign (+1 or -1).
    float4 world_tangent : TEXCOORD5;
};

struct GBufferOutput
{
    float4 normal           : SV_Target0; // oct(shading normal), oct(geometric normal)
    float4 albedo           : SV_Target1; // base color (rgba)
    float2 roughness_metal  : SV_Target2; // (roughness, metallic)
};

[shader("vertex")]
V2F vs_main(uint vertexId : SV_VertexID, uint instanceId : SV_InstanceID)
{
    DrawItem item = draw_items[instanceId];
    Vertex v = vertices[vertexId];
    float4 world_pos = mul(item.model_matrix, float4(v.position, 1.0));

    V2F o;
    o.position     = mul(pc.viewproj, world_pos);
    o.uv           = v.uv;
    o.material_id  = item.material.x;
    o.world_normal = normalize(mul((float3x3)item.normal_matrix, v.normal));
    o.world_pos    = world_pos.xyz;
    o.vert_color   = v.color;
    // Tangents are directions in the surface plane, so transform them by the
    // model matrix, not the inverse-transpose normal matrix.
    float handedness = determinant((float3x3)item.model_matrix) < 0.0 ? -1.0 : 1.0;
    o.world_tangent = float4(mul((float3x3)item.model_matrix, v.tangent.xyz), v.tangent.w * handedness);
    return o;
}

[shader("pixel")]
GBufferOutput ps_main(V2F input)
{
    const uint INVALID = 0xFFFFFFFFu;

    float4 base_color = input.vert_color;
    float  roughness  = 0.5;
    float  metallic   = 0.0;
    float  alpha_cutoff = 0.0;

    float3 interp_N = normalize(input.world_normal);
    // True geometric normal from the world-space surface derivatives. This is
    // stable per triangle and is the right normal to use for RT biasing.
    float3 dpdx = ddx(input.world_pos);
    float3 dpdy = ddy(input.world_pos);
    float3 face_N = cross(dpdx, dpdy);
    float face_len2 = dot(face_N, face_N);
    float3 gN = face_len2 > 1e-12 ? normalize(face_N) : interp_N;
    if (dot(gN, interp_N) < 0.0)
        gN = -gN;

    // Shading normal — may be perturbed by normal map below.
    float3 N = interp_N;

    if (input.material_id != INVALID)
    {
        PbrMaterial mat = materials[input.material_id];
        base_color *= mat.base_color;
        roughness   = mat.roughness;
        metallic    = mat.metallness;
        alpha_cutoff = mat.alpha_cutoff;

        uint albedoId      = mat.albedo_tex.x;
        uint albedoSampler = mat.albedo_sampler.x;
        if (albedoId != INVALID && albedoSampler != INVALID)
        {
            int texIdx  = clamp((int)albedoId,     0, 1023);
            int sampIdx = clamp((int)albedoSampler, 0, 1023);
            base_color *= textures[texIdx].Sample(samplers[sampIdx], input.uv);
        }

        // Metallic-roughness texture (glTF standard: G=roughness, B=metallic)
        uint mrId      = mat.metallic_roughness_tex.x;
        uint mrSampler = mat.metallic_roughness_sampler.x;
        if (mrId != INVALID && mrSampler != INVALID)
        {
            int texIdx  = clamp((int)mrId,      0, 1023);
            int sampIdx = clamp((int)mrSampler,  0, 1023);
            float4 mr = textures[texIdx].Sample(samplers[sampIdx], input.uv);
            roughness = roughness * mr.g;
            metallic  = metallic * mr.b;
        }

        // Normal map — glTF tangent-space, stored as RGB (xy from [0,1]->[-1,1], z derived).
        uint normalId      = mat.normal_tex.x;
        uint normalSampler = mat.normal_sampler.x;
        if (normalId != INVALID && normalSampler != INVALID)
        {
            int texIdx  = clamp((int)normalId,      0, 1023);
            int sampIdx = clamp((int)normalSampler,  0, 1023);
            float3 ts_normal = textures[texIdx].Sample(samplers[sampIdx], input.uv).xyz;
            // Decode from [0,1] to [-1,1].
            ts_normal = ts_normal * 2.0 - 1.0;

            // Re-orthogonalize TBN via Gram-Schmidt to handle interpolation drift.
            float3 T = normalize(input.world_tangent.xyz);
            float3 Nv = interp_N;
            T = normalize(T - dot(T, Nv) * Nv);
            float3 B = cross(Nv, T) * input.world_tangent.w;

            // Transform tangent-space normal to world space.
            N = normalize(ts_normal.x * T + ts_normal.y * B + ts_normal.z * Nv);
        }
    }

    roughness = max(roughness, 0.04);
    metallic = saturate(metallic);

    if (GBUF_ALPHA_TEST != 0u)
    {
        float alpha_grad = length(float2(ddx(base_color.a), ddy(base_color.a)));
        float adjusted_cutoff = alpha_cutoff + 0.5 * alpha_grad;
        if (base_color.a < adjusted_cutoff)
            discard;
    }

    GBufferOutput o;
    // Octahedral dual-normal encoding in RGBA16F:
    //   xy = oct(shading normal)   — used for PBR shading
    //   zw = oct(geometric/face normal) — used for shadow ray bias in RT pass
    o.normal          = float4(oct_encode(N), oct_encode(gN));
    o.albedo          = base_color;
    o.roughness_metal = float2(roughness, metallic);
    return o;
}
