// rt.hlsl — all RT entry points: raygeneration, miss, closesthit.
// Compiled as a single lib_6_6 SPIR-V module by build.rs.

#include "common.hlsli"

// ---------------------------------------------------------------------------
// Push constants (must match RtPushConstants in rt.rs)
// ---------------------------------------------------------------------------
struct RtPushConstants
{
    column_major float4x4 inv_viewproj;
    float4 camera_pos;
    uint   width;
    uint   height;
    uint   _pad0;
    uint   _pad1;
};

[[vk::push_constant]] ConstantBuffer<RtPushConstants> pc;

// ---------------------------------------------------------------------------
// Meta bindings (set=0) — indices match META_BUFFER_BINDING_* in resources/mod.rs
// ---------------------------------------------------------------------------
[[vk::binding(0, 0)]] StructuredBuffer<Vertex>      vb;
[[vk::binding(1, 0)]] StructuredBuffer<PbrMaterial>  materials;
[[vk::binding(2, 0)]] Texture2D                      textures[1024];
[[vk::binding(3, 0)]] StructuredBuffer<DrawItem>     dib;
[[vk::binding(4, 0)]] SamplerState                   samplers[1024];
[[vk::binding(5, 0)]] ConstantBuffer<Skybox>         skybox;
[[vk::binding(6, 0)]] StructuredBuffer<uint>         ib;

// TLAS lives in the meta set (set=0) at META_BUFFER_BINDING_TLAS = 7.
[[vk::binding(7, 0)]] RaytracingAccelerationStructure tlas;

// ---------------------------------------------------------------------------
// RT-private bindings (set=1)
// ---------------------------------------------------------------------------
[[vk::binding(0, 1)]] [[vk::image_format("bgra8")]] RWTexture2D<float4> color_out;

// ---------------------------------------------------------------------------
// Ray payload
// ---------------------------------------------------------------------------
struct RayPayload
{
    float4 color;
};

// ---------------------------------------------------------------------------
// GGX / Cook-Torrance BRDF helpers (used by closesthit)
// ---------------------------------------------------------------------------
static const float PI = 3.14159265359;

float D_GGX(float NdotH, float roughness)
{
    float a  = roughness * roughness;
    float a2 = a * a;
    float d  = NdotH * NdotH * (a2 - 1.0) + 1.0;
    return a2 / max(PI * d * d, 1e-7);
}

float G_SchlickGGX(float NdotV, float roughness)
{
    float r = roughness + 1.0;
    float k = (r * r) / 8.0;
    return NdotV / max(NdotV * (1.0 - k) + k, 1e-7);
}

float G_Smith(float NdotV, float NdotL, float roughness)
{
    return G_SchlickGGX(max(NdotV, 1e-4), roughness)
         * G_SchlickGGX(max(NdotL, 1e-4), roughness);
}

float3 F_Schlick(float cosTheta, float3 F0)
{
    return F0 + (1.0 - F0) * pow(saturate(1.0 - cosTheta), 5.0);
}

float3 pbr_direct_light(
    float3 N,
    float3 V,
    float3 L,
    float3 albedo,
    float  metallic,
    float  roughness,
    float3 light_color)
{
    float NdotL = saturate(dot(N, L));
    if (NdotL <= 0.0)
        return float3(0.0, 0.0, 0.0);

    float3 H     = normalize(V + L);
    float  NdotV = max(dot(N, V), 1e-4);
    float  NdotH = saturate(dot(N, H));
    float  HdotV = saturate(dot(H, V));

    float3 F0 = lerp(float3(0.04, 0.04, 0.04), albedo, metallic);

    float  D = D_GGX(NdotH, roughness);
    float  G = G_Smith(NdotV, NdotL, roughness);
    float3 F = F_Schlick(HdotV, F0);

    float3 specular = (D * G * F) / max(4.0 * NdotV * NdotL, 1e-4);

    float3 kD      = (1.0 - F) * (1.0 - metallic);
    float3 diffuse = kD * albedo / PI;

    return (diffuse + specular) * NdotL * light_color;
}

// ---------------------------------------------------------------------------
// Raygen entry point
// ---------------------------------------------------------------------------
[shader("raygeneration")]
void raygen_main()
{
    uint2 pixel = DispatchRaysIndex().xy;

    if (pixel.x >= pc.width || pixel.y >= pc.height)
        return;

    // Map pixel centre to NDC (Vulkan NDC has Y pointing down).
    float2 uv  = (float2(pixel) + 0.5) / float2(pc.width, pc.height);
    float2 ndc = uv * 2.0 - 1.0;

    // Reconstruct ray from inverse view-projection.
    // Reverse-Z: NDC Z=1 at near plane, Z=0 at infinity.
    float4 near_world = mul(pc.inv_viewproj, float4(ndc, 1.0, 1.0));
    float4 mid_world  = mul(pc.inv_viewproj, float4(ndc, 0.5, 1.0));
    near_world /= near_world.w;
    mid_world  /= mid_world.w;

    float3 ray_origin    = near_world.xyz;
    float3 ray_direction = normalize(mid_world.xyz - near_world.xyz);

    RayDesc ray;
    ray.Origin    = ray_origin;
    ray.Direction = ray_direction;
    ray.TMin      = 0.001;
    ray.TMax      = 10000.0;

    RayPayload payload;
    payload.color = float4(0.0, 0.0, 0.0, 1.0);

    TraceRay(
        tlas,
        RAY_FLAG_NONE,
        0xFF,  // instance mask
        0,     // hit group SBT offset
        1,     // hit group SBT stride
        0,     // miss SBT index
        ray,
        payload
    );

    color_out[pixel] = payload.color;
}

// ---------------------------------------------------------------------------
// Miss entry point
// ---------------------------------------------------------------------------
[shader("miss")]
void miss_main(inout RayPayload payload)
{
    const uint INVALID = 0xFFFFFFFFu;

    float3 dir = normalize(WorldRayDirection());

    if (skybox.tex != INVALID && skybox.sampler != INVALID)
    {
        int texIndex  = clamp((int)skybox.tex,    0, 1023);
        int sampIndex = clamp((int)skybox.sampler, 0, 1023);
        float2 uv = dir_to_equirect_uv(dir);
        float3 hdr = textures[texIndex].SampleLevel(samplers[sampIndex], uv, 0).rgb;
        hdr *= skybox.exposure;
        float3 color = saturate(tonemap_reinhard(hdr));
        payload.color = float4(color, 1.0);
    }
    else
    {
        float t = 0.5 * (dir.y + 1.0);
        float3 color = lerp(float3(0.3, 0.3, 0.3), float3(0.1, 0.2, 0.5), t);
        payload.color = float4(color, 1.0);
    }
}

// ---------------------------------------------------------------------------
// Closest-hit entry point
// ---------------------------------------------------------------------------
[shader("closesthit")]
void hit_main(inout RayPayload payload, BuiltInTriangleIntersectionAttributes attr)
{
    const uint INVALID = 0xFFFFFFFFu;

    // InstanceID() == draw_item_index set as instance_custom_index in build_tlas_for_items.
    uint draw_item_idx = InstanceID();
    DrawItem item      = dib[draw_item_idx];
    uint mat_idx       = item.material.x;

    // DrawItem.mesh_offsets packed by ResourceManager::pack_mesh_offsets():
    //   .x = vertex_offset, .y = index_offset
    uint vertex_offset = item.mesh_offsets.x;
    uint index_offset  = item.mesh_offsets.y;

    // Barycentric weights.
    float w1 = attr.barycentrics.x;
    float w2 = attr.barycentrics.y;
    float w0 = 1.0 - w1 - w2;

    // Look up indices of the hit triangle.
    uint prim_idx = PrimitiveIndex();
    uint i0 = ib[index_offset + prim_idx * 3 + 0];
    uint i1 = ib[index_offset + prim_idx * 3 + 1];
    uint i2 = ib[index_offset + prim_idx * 3 + 2];

    Vertex v0 = vb[vertex_offset + i0];
    Vertex v1 = vb[vertex_offset + i1];
    Vertex v2 = vb[vertex_offset + i2];

    // Interpolate vertex attributes.
    float3 local_normal = normalize(v0.normal * w0 + v1.normal * w1 + v2.normal * w2);
    float2 uv           = v0.uv * w0 + v1.uv * w1 + v2.uv * w2;
    float4 vert_color   = v0.color * w0 + v1.color * w1 + v2.color * w2;

    // Transform normal to world space.
    float3 world_normal = normalize(mul((float3x3)item.normal_matrix, local_normal));

    // Two-sided shading.
    if (dot(world_normal, -WorldRayDirection()) < 0.0)
        world_normal = -world_normal;

    // Material lookup.
    float4 base_color = vert_color;
    float  metallic   = 0.0;
    float  roughness  = 0.5;

    if (mat_idx != INVALID)
    {
        PbrMaterial mat = materials[mat_idx];
        base_color *= mat.base_color;
        metallic    = mat.metallness;
        roughness   = mat.roughness;

        uint albedoId      = mat.albedo_tex.x;
        uint albedoSampler = mat.albedo_sampler.x;
        if (albedoId != INVALID && albedoSampler != INVALID)
        {
            int texIdx  = clamp((int)albedoId,      0, 1023);
            int sampIdx = clamp((int)albedoSampler,  0, 1023);
            base_color *= textures[texIdx].SampleLevel(samplers[sampIdx], uv, 0);
        }
    }

    float3 albedo = base_color.rgb;
    roughness = max(roughness, 0.04);

    // Direct sun lighting.
    float3 hit_pos   = WorldRayOrigin() + WorldRayDirection() * RayTCurrent();
    float3 V         = normalize(pc.camera_pos.xyz - hit_pos);
    float3 sun_dir   = normalize(float3(0.4, 1.0, 0.3));
    float3 sun_color = float3(1.0, 0.95, 0.85) * 3.0;

    float3 direct = pbr_direct_light(world_normal, V, sun_dir, albedo, metallic, roughness, sun_color);

    // Sky ambient.
    float3 ambient = float3(0.03, 0.03, 0.03) * albedo;
    if (skybox.tex != INVALID && skybox.sampler != INVALID)
    {
        int texIndex  = clamp((int)skybox.tex,    0, 1023);
        int sampIndex = clamp((int)skybox.sampler, 0, 1023);
        float3 hdr = textures[texIndex].SampleLevel(samplers[sampIndex],
                         dir_to_equirect_uv(world_normal), 0).rgb;
        hdr *= skybox.exposure;
        float3 amb = saturate(tonemap_reinhard(hdr));
        ambient = albedo * (skybox.ambient_floor + (1.0 - skybox.ambient_floor) * amb);
    }

    float3 final_color = ambient + direct;

    // Gamma correction: linear -> sRGB.
    final_color = linear_to_srgb(final_color);

    payload.color = float4(final_color, base_color.a);
}
