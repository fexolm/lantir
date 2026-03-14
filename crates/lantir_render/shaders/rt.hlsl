// rt.hlsl — Deferred lighting + shadow ray tracing pass.
// Reads GBuffer (normal, albedo, roughness/metallic, depth) written by the geometry pass.
// Traces one shadow ray per pixel toward the sun. Outputs a fully lit HDR image.
//
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

// TLAS — for shadow ray tracing
[[vk::binding(7, 0)]] RaytracingAccelerationStructure tlas;

// ---------------------------------------------------------------------------
// RT-private bindings (set=1)
// ---------------------------------------------------------------------------
[[vk::binding(0, 1)]] [[vk::image_format("bgra8")]] RWTexture2D<float4> color_out;
[[vk::binding(1, 1)]] Texture2D<float4> gbuf_normal;          // encoded world normal (xyz*0.5+0.5, w unused)
[[vk::binding(2, 1)]] Texture2D<float4> gbuf_albedo;          // base color RGBA
[[vk::binding(3, 1)]] Texture2D<float2> gbuf_roughness_metal; // (roughness, metallic)
[[vk::binding(4, 1)]] Texture2D<float>  gbuf_depth;           // reverse-Z depth (1=near, 0=far)

// ---------------------------------------------------------------------------
// GGX / Cook-Torrance BRDF helpers
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
    float3 N, float3 V, float3 L,
    float3 albedo, float metallic, float roughness,
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
    float3 kD       = (1.0 - F) * (1.0 - metallic);
    float3 diffuse  = kD * albedo / PI;

    return (diffuse + specular) * NdotL * light_color;
}

// Reconstruct world-space position from a pixel's reverse-Z depth value.
// ndc_xy: pixel NDC xy in [-1,1]. depth: reverse-Z depth (0=far, 1=near).
float3 reconstruct_world_pos(float2 ndc_xy, float depth, float4x4 inv_viewproj)
{
    float4 clip = float4(ndc_xy, depth, 1.0);
    float4 world = mul(inv_viewproj, clip);
    return world.xyz / world.w;
}

// ---------------------------------------------------------------------------
// Shadow ray payload — visibility flag only
// ---------------------------------------------------------------------------
struct ShadowPayload
{
    float visibility; // 1.0 = lit, 0.0 = shadowed
};

// ---------------------------------------------------------------------------
// Raygen: read GBuffer, reconstruct world pos from depth, shade + trace shadow
// ---------------------------------------------------------------------------
[shader("raygeneration")]
void raygen_main()
{
    uint2 pixel = DispatchRaysIndex().xy;
    if (pixel.x >= pc.width || pixel.y >= pc.height)
        return;

    const uint INVALID = 0xFFFFFFFFu;

    // Sample GBuffer at this pixel (integer load, no filtering needed)
    float4 gbuf_n  = gbuf_normal.Load(int3(pixel, 0));
    float4 gbuf_a  = gbuf_albedo.Load(int3(pixel, 0));
    float2 gbuf_rm = gbuf_roughness_metal.Load(int3(pixel, 0));
    float  depth   = gbuf_depth.Load(int3(pixel, 0));

    // Compute pixel NDC coordinates
    float2 uv  = (float2(pixel) + 0.5) / float2(pc.width, pc.height);
    float2 ndc = uv * 2.0 - 1.0;

    // Sky pixel: normal encodes (0,0,0) in [0,1] space → stored value ~(0.5,0.5,0.5),
    // but we cleared gbuf_normal to (0,0,0,0) — so detect with w==0 or zero normal length.
    // Depth == 0.0 means far plane (reverse-Z) → sky.
    if (depth <= 0.0 || dot(gbuf_n.xyz, gbuf_n.xyz) < 0.001)
    {
        // Sky pixel — sample skybox in the view direction
        float4 near_w = mul(pc.inv_viewproj, float4(ndc, 1.0, 1.0));
        float4 far_w  = mul(pc.inv_viewproj, float4(ndc, 0.0, 1.0));
        near_w /= near_w.w;
        far_w  /= far_w.w;
        float3 ray_dir = normalize(far_w.xyz - near_w.xyz);

        if (skybox.tex != INVALID && skybox.sampler != INVALID)
        {
            int texIndex  = clamp((int)skybox.tex,    0, 1023);
            int sampIndex = clamp((int)skybox.sampler, 0, 1023);
            float3 hdr = textures[texIndex].SampleLevel(samplers[sampIndex],
                             dir_to_equirect_uv(ray_dir), 0).rgb;
            hdr *= skybox.exposure;
            color_out[pixel] = float4(saturate(tonemap_reinhard(hdr)), 1.0);
        }
        else
        {
            float t = 0.5 * (ray_dir.y + 1.0);
            color_out[pixel] = float4(lerp(float3(0.3, 0.3, 0.3), float3(0.1, 0.2, 0.5), t), 1.0);
        }
        return;
    }

    // Decode GBuffer
    float3 N        = normalize(gbuf_n.xyz * 2.0 - 1.0);
    float3 albedo   = gbuf_a.rgb;
    float  roughness = max(gbuf_rm.x, 0.04);
    float  metallic  = gbuf_rm.y;

    // Reconstruct world-space position from reverse-Z depth
    float3 hit_pos = reconstruct_world_pos(ndc, depth, pc.inv_viewproj);

    float3 V = normalize(pc.camera_pos.xyz - hit_pos);

    // Sun parameters (fixed directional light)
    float3 sun_dir = normalize(float3(0.1, 1.0, 0.0));
    float3 sun_color = float3(1.0, 0.95, 0.85) * 3.0;

    // Shadow ray toward the sun
    float shadow_visibility = 1.0;
    {
        RayDesc shadow_ray;
        shadow_ray.Origin    = hit_pos + N * 0.005; // normal-offset bias
        shadow_ray.Direction = sun_dir;
        shadow_ray.TMin      = 0.001;
        shadow_ray.TMax      = 10000.0;

        ShadowPayload shadow;
        shadow.visibility = 0.0; // default: shadowed; miss shader sets 1.0 if unoccluded

        TraceRay(
            tlas,
            RAY_FLAG_ACCEPT_FIRST_HIT_AND_END_SEARCH | RAY_FLAG_SKIP_CLOSEST_HIT_SHADER,
            0xFF,
            0,   // sbt_offset (hit group 0, but skipped by flag)
            1,   // sbt_stride
            0,   // miss index 0 → shadow_miss_main
            shadow_ray,
            shadow
        );
        shadow_visibility = shadow.visibility;
    }

    // PBR shading
    float3 direct = shadow_visibility
        * pbr_direct_light(N, V, sun_dir, albedo, metallic, roughness, sun_color);

    // Skybox ambient (sample along surface normal)
    float3 ambient = float3(0.03, 0.03, 0.03) * albedo;
    if (skybox.tex != INVALID && skybox.sampler != INVALID)
    {
        int texIndex  = clamp((int)skybox.tex,    0, 1023);
        int sampIndex = clamp((int)skybox.sampler, 0, 1023);
        float3 hdr = textures[texIndex].SampleLevel(samplers[sampIndex],
                         dir_to_equirect_uv(N), 0).rgb;
        hdr *= skybox.exposure;
        float3 amb = saturate(tonemap_reinhard(hdr));
        ambient = albedo * (skybox.ambient_floor + (1.0 - skybox.ambient_floor) * amb);
    }

    float3 final_color = linear_to_srgb(ambient + direct);
    color_out[pixel] = float4(final_color, 1.0);
}

// ---------------------------------------------------------------------------
// Shadow miss — ray reached the light unoccluded: lit
// ---------------------------------------------------------------------------
[shader("miss")]
void shadow_miss_main(inout ShadowPayload payload)
{
    payload.visibility = 1.0;
}

// ---------------------------------------------------------------------------
// Primary miss — no geometry at this pixel (unused in this deferred pipeline,
// but required as the SBT's second miss entry for correctness)
// ---------------------------------------------------------------------------
[shader("miss")]
void primary_miss_main(inout ShadowPayload payload)
{
    payload.visibility = 0.0;
}

// ---------------------------------------------------------------------------
// Primary closest-hit — unused in deferred pipeline (shadow rays skip CHit).
// Needed in the SBT hit region so the pipeline has a valid hit group.
// ---------------------------------------------------------------------------
[shader("closesthit")]
void primary_hit_main(inout ShadowPayload payload, BuiltInTriangleIntersectionAttributes attr)
{
    // Shadow ray hit geometry → occluded
    payload.visibility = 0.0;
}
