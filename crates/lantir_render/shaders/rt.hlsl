// rt.hlsl -- Simplified deferred lighting + ray-traced shadow pass.
// Compatible with the current rt.rs pipeline and keeps the required entry points.

#include "common.hlsli"
#include "brdf.hlsli"

struct RtPushConstants
{
    column_major float4x4 inv_viewproj;
    float4 camera_pos;
    uint   width;
    uint   height;
    uint   frame_index;
};

[[vk::push_constant]] ConstantBuffer<RtPushConstants> pc;

// ---------------------------------------------------------------------------
// Meta bindings (set = 0)
// ---------------------------------------------------------------------------
[[vk::binding(0, 0)]] StructuredBuffer<Vertex>       vertices;
[[vk::binding(1, 0)]] StructuredBuffer<PbrMaterial>  materials;
[[vk::binding(2, 0)]] Texture2D                      textures[1024];
[[vk::binding(3, 0)]] StructuredBuffer<DrawItem>     draw_items;
[[vk::binding(4, 0)]] SamplerState                   samplers[1024];
[[vk::binding(5, 0)]] ConstantBuffer<Skybox>         skybox;
[[vk::binding(6, 0)]] StructuredBuffer<uint>         indices;
[[vk::binding(7, 0)]] RaytracingAccelerationStructure tlas;

// ---------------------------------------------------------------------------
// RT-private bindings (set = 1)
// ---------------------------------------------------------------------------
[[vk::binding(0, 1)]] [[vk::image_format("bgra8")]] RWTexture2D<float4> color_out;
[[vk::binding(1, 1)]] Texture2D<float4> gbuf_normal;
[[vk::binding(2, 1)]] Texture2D<float4> gbuf_albedo;
[[vk::binding(4, 1)]] Texture2D<float>  gbuf_depth;

static const uint  INVALID = 0xFFFFFFFFu;
static const uint  RAY_MASK_ALL = 0xFFu;

// SBT selection constants. These must match the group layout in rt.rs:
//   miss 0 = shadow_miss_main
//   miss 1 = primary_miss_main
//   miss 2 = bounce_miss_main
//   hit  0 = primary_hit_main
//   hit  1 = bounce_hit_main
static const uint SHADOW_HIT_GROUP_OFFSET = 0u;
static const uint SHADOW_HIT_GROUP_STRIDE = 1u;
static const uint SHADOW_MISS_INDEX       = 0u;

static const uint BOUNCE_HIT_GROUP_OFFSET = 1u;
static const uint BOUNCE_HIT_GROUP_STRIDE = 1u;
static const uint BOUNCE_MISS_INDEX       = 2u;

static const float RAYGEN_NORMAL_OFFSET       = 0.001;

#define FLOAT_SCALE (1.0 / 65536.0)
#define INT_SCALE (256.0)
#define ORIGIN_THRESHOLD (1.0 / 32.0)

#include "raytrace_utils.hlsli"

struct ShadowPayload
{
    float visibility; // 1 = lit, 0 = occluded
};

struct BouncePayload
{
    float3 hit_albedo;
    float3 world_pos;
    float3 N_shading;
    float  roughness;
    float  metallic;
    bool   hit;
    float3 sky_color;
};


float3 get_sun_dir()
{
    return normalize(float3(0.0, 1.0, 0.1));
}

float3 get_sun_color()
{
    return float3(1.0, 0.95, 0.85) * 4.0;
}

float trace_shadow_from_origin(float3 ray_origin, float3 sun_dir)
{
    uint2 pixel = DispatchRaysIndex().xy;
    uint seed = pixel.x + pixel.y * pc.width + pc.frame_index * 987654;

    RayDesc ray;
    ray.Origin    = ray_origin;
    ray.Direction = add_jitter(sun_dir, 0.01, seed);
    ray.TMin      = 0.001;
    ray.TMax      = 10000.0;

    ShadowPayload payload;
    payload.visibility = 0.0;

    TraceRay(
        tlas,
        RAY_FLAG_ACCEPT_FIRST_HIT_AND_END_SEARCH | RAY_FLAG_SKIP_CLOSEST_HIT_SHADER,
        RAY_MASK_ALL,
        SHADOW_HIT_GROUP_OFFSET,
        SHADOW_HIT_GROUP_STRIDE,
        SHADOW_MISS_INDEX,
        ray,
        payload
    );

    return payload.visibility;
}

float3 shade_surface_from_origin(float3 albedo, float3 N, float3 view_dir, float roughness, float metallic, float3 shadow_origin)
{
    float3 sun_dir = get_sun_dir();

    float3 H = normalize(sun_dir + view_dir);
    float NdotH = saturate(dot(N, H));
    float NdotV = saturate(dot(N, view_dir));
    float NdotL = saturate(dot(N, sun_dir));
    float VdotH = saturate(dot(view_dir, H));
    float3 R = reflect(-view_dir, N); // Rreflection direction for the view ray

    // Fresnel reflectance at normal incidence (how much light is reflected vs refracted) 
    float3 F0 = lerp(float3(0.04, 0.04, 0.04), albedo, metallic);

    // Fresnel term using Schlick's approximation. Provides a smooth interpolation between F0 at grazing angles and near-zero reflectance at normal incidence.
    float3 F  = fresnel_schlick(VdotH, F0);

    // Normal distribution function (NDF) using GGX/Trowbridge-Reitz. Models the microfacet orientation distribution, controlling the sharpness of specular highlights.
    float3 D = distribution_ggx(NdotH, roughness);

    // Geometry function using Schlick-GGX. Models shadowing and masking of microfacets, affecting the visibility of specular reflections.
    float  G  = geometry_smith(NdotV, NdotL, roughness);

    // Cook-Torrance microfacet BRDF combining the above terms. Represents the specular reflection component of the surface response.
    float3 specular = (D * F * G) / max(4.0 * NdotV * NdotL, 1e-6);

    float3 kD = (1.0 - F) * (1.0 - metallic); // Diffuse reflection component, reduced by Fresnel reflectance and metallic factor.

    float3 diffuse = kD * albedo / PI; // Lambertian diffuse term, normalized by PI.

    float shadow = trace_shadow_from_origin(shadow_origin, sun_dir);

    float3 radiance = get_sun_color();

    // Final outgoing radiance is the sum of diffuse and specular contributions, modulated by the incoming radiance, angle of incidence (NdotL), and shadow visibility.
    float3 direct = (diffuse + specular) * radiance * NdotL * shadow;


    float3 irradiance = evaluate_sh9(N, skybox.sh);
    float3 diffuse_ibl = kD * albedo * irradiance; // Diffuse image-based lighting from the environment, modulated by the diffuse albedo and Fresnel term.

    float3 env_specular = sample_sky(R, roughness * MAX_ENV_LOD);
    float3 F_ibl = fresnel_schlick_roughness(NdotV, F0, roughness);
    float3 specular_ibl = F_ibl * env_specular; // Specular image-based lighting from the environment, modulated by the Fresnel term.

    return direct + diffuse_ibl + specular_ibl;
}

[shader("raygeneration")]
void raygen_main()
{
    uint2 pixel = DispatchRaysIndex().xy;
    if (pixel.x >= pc.width || pixel.y >= pc.height)
        return;

    float4 gbuf_n  = gbuf_normal.Load(int3(pixel, 0));
    float4 gbuf_a  = gbuf_albedo.Load(int3(pixel, 0));
    float  depth   = gbuf_depth.Load(int3(pixel, 0));

    float2 uv  = pixel_to_uv(pixel);
    float2 ndc = pixel_to_ndc(pixel);
    float2 inv_res = 1.0 / float2(pc.width, pc.height);

    if (depth <= 0.0)
    {
        float3 sky_hdr = sample_sky(reconstruct_world_ray_dir(ndc), 0);
        color_out[pixel] = float4(linear_to_srgb(tonemap_aces(sky_hdr)), 1.0);
        return;
    }

    float3 normals       = oct_decode(gbuf_n.xy);
    float roughness  = gbuf_n.z;
    float metallic   = gbuf_n.w;
    float3 albedo  = gbuf_a.rgb;
    float3 world_pos = reconstruct_world_pos_from_uv(uv, depth);
    float3 shadow_origin = add_bias(world_pos + normals * RAYGEN_NORMAL_OFFSET, normals);
    float3 view_dir = normalize(pc.camera_pos.xyz - world_pos);

    float3 final_hdr = shade_surface_from_origin(albedo, normals, view_dir, roughness, metallic, shadow_origin);
    color_out[pixel] = float4(linear_to_srgb(tonemap_aces(final_hdr)), 1.0);
}

[shader("miss")]
void shadow_miss_main(inout ShadowPayload payload)
{
    payload.visibility = 1.0;
}

[shader("miss")]
void primary_miss_main(inout ShadowPayload payload)
{
    payload.visibility = 0.0;
}

[shader("miss")]
void bounce_miss_main(inout BouncePayload payload)
{
    payload.hit = false;
    payload.sky_color = sample_sky(WorldRayDirection(), 0);
}

[shader("closesthit")]
void primary_hit_main(inout ShadowPayload payload, BuiltInTriangleIntersectionAttributes attr)
{
    payload.visibility = 0.0;
}

[shader("closesthit")]
void bounce_hit_main(inout BouncePayload payload, BuiltInTriangleIntersectionAttributes attr)
{
    uint instance_id = InstanceIndex();
    uint prim_id     = PrimitiveIndex();

    DrawItem di = draw_items[instance_id];

    uint vertex_offset = di.mesh_offsets.x;
    uint index_offset  = di.mesh_offsets.y;

    uint i0 = indices[index_offset + prim_id * 3 + 0];
    uint i1 = indices[index_offset + prim_id * 3 + 1];
    uint i2 = indices[index_offset + prim_id * 3 + 2];

    Vertex v0 = vertices[vertex_offset + i0];
    Vertex v1 = vertices[vertex_offset + i1];
    Vertex v2 = vertices[vertex_offset + i2];

    float2 bary = attr.barycentrics;
    float  w0   = 1.0 - bary.x - bary.y;
    float  w1   = bary.x;
    float  w2   = bary.y;

    float2 uv = v0.uv * w0 + v1.uv * w1 + v2.uv * w2;
    float4 color = v0.color * w0 + v1.color * w1 + v2.color * w2;

    float3 local_interp_N = normalize(v0.normal * w0 + v1.normal * w1 + v2.normal * w2);
    float3 world_shading_N = normalize(mul((float3x3)di.normal_matrix, local_interp_N));

    float3 world_pos = WorldRayOrigin() + WorldRayDirection() * RayTCurrent();

    uint mat_idx = di.material.x;
    PbrMaterial mat = materials[mat_idx];

    float3 hit_albedo = mat.base_color.rgb * color.rgb;
    uint alb_tex = mat.albedo_tex.x;
    uint alb_smp = mat.albedo_sampler.x;
    if (alb_tex != INVALID && alb_smp != INVALID)
    {
        int texIdx  = clamp((int)alb_tex, 0, 1023);
        int sampIdx = clamp((int)alb_smp, 0, 1023);
        hit_albedo *= textures[texIdx].SampleLevel(samplers[sampIdx], uv, 0).rgb;
    }

    float hit_roughness = max(mat.roughness, 0.04);
    float hit_metallic  = saturate(mat.metallness);
    uint rm_tex = mat.metallic_roughness_tex.x;
    uint rm_smp = mat.metallic_roughness_sampler.x;
    if (rm_tex != INVALID && rm_smp != INVALID)
    {
        int texIdx  = clamp((int)rm_tex, 0, 1023);
        int sampIdx = clamp((int)rm_smp, 0, 1023);
        float2 rm = textures[texIdx].SampleLevel(samplers[sampIdx], uv, 0).gb;
        hit_roughness = max(mat.roughness * rm.x, 0.04);
        hit_metallic  = saturate(mat.metallness * rm.y);
    }

    payload.hit        = true;
    payload.hit_albedo = hit_albedo;
    payload.world_pos  = world_pos;
    payload.N_shading  = world_shading_N;
    payload.roughness  = hit_roughness;
    payload.metallic   = hit_metallic;
    payload.sky_color  = float3(0, 0, 0);
}
