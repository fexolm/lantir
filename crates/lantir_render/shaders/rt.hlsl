// rt.hlsl -- Simplified deferred lighting + ray-traced shadow/GI pass.
// Compatible with the current rt.rs pipeline:
// - keeps the same entry points
// - keeps the same 2-bounce structure
// - removes most of the PBR/IBL complexity so the shader is easier to read

#include "common.hlsli"

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
[[vk::binding(3, 1)]] Texture2D<float2> gbuf_roughness_metal; // kept for future PBR work
[[vk::binding(4, 1)]] Texture2D<float>  gbuf_depth;

static const uint  INVALID = 0xFFFFFFFFu;
static const float PI = 3.14159265359;
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

struct ShadowPayload
{
    float visibility; // 1 = lit, 0 = occluded
};

struct BouncePayload
{
    float3 hit_albedo;
    float3 world_pos;
    float3 N_shading;
    float3 N_geom;
    float  roughness;
    float  metallic;
    bool   hit;
    float3 sky_color;
};

float3 sample_sky(float3 dir)
{
    dir = normalize(dir);

    if (skybox.tex != INVALID && skybox.sampler != INVALID)
    {
        int texIdx  = clamp((int)skybox.tex, 0, 1023);
        int sampIdx = clamp((int)skybox.sampler, 0, 1023);
        return textures[texIdx].SampleLevel(
            samplers[sampIdx],
            dir_to_equirect_uv(dir),
            0
        ).rgb * skybox.exposure;
    }

    float t = saturate(0.5 * (dir.y + 1.0));
    return lerp(float3(0.3, 0.3, 0.35), float3(0.15, 0.35, 0.8), t);
}

float3 reconstruct_world_pos(float2 ndc_xy, float depth)
{
    float4 clip = float4(ndc_xy, depth, 1.0);
    float4 world = mul(pc.inv_viewproj, clip);
    return world.xyz / world.w;
}

float3 reconstruct_world_ray_dir(float2 ndc_xy)
{
    float4 world = mul(pc.inv_viewproj, float4(ndc_xy, 0.0, 1.0));
    float3 world_pos = world.xyz / max(world.w, 1e-6);
    return normalize(world_pos - pc.camera_pos.xyz);
}

uint hash_pcg(uint v)
{
    uint state = v * 747796405u + 2891336453u;
    uint word  = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

float2 hash2(uint2 seed)
{
    uint h0 = hash_pcg(seed.x ^ (seed.y * 1664525u + 1013904223u));
    uint h1 = hash_pcg(h0 ^ seed.y);
    return float2(h0, h1) * (1.0 / 4294967296.0);
}

float3 cosine_sample_hemisphere(float3 N, float2 rand)
{
    float r   = sqrt(rand.x);
    float phi = 2.0 * PI * rand.y;
    float x   = r * cos(phi);
    float y   = r * sin(phi);
    float z   = sqrt(max(0.0, 1.0 - rand.x));

    float3 up = abs(N.y) < 0.999 ? float3(0, 1, 0) : float3(1, 0, 0);
    float3 T  = normalize(cross(up, N));
    float3 B  = cross(N, T);
    return normalize(x * T + y * B + z * N);
}

float compute_bias(float3 world_pos)
{
    float view_dist = length(pc.camera_pos.xyz - world_pos);
    return max(0.001, view_dist * 0.0005);
}

BouncePayload make_default_bounce_payload()
{
    BouncePayload payload;
    payload.hit        = false;
    payload.hit_albedo = float3(0, 0, 0);
    payload.world_pos  = float3(0, 0, 0);
    payload.N_shading  = float3(0, 1, 0);
    payload.N_geom     = float3(0, 1, 0);
    payload.roughness  = 1.0;
    payload.metallic   = 0.0;
    payload.sky_color  = float3(0, 0, 0);
    return payload;
}

float trace_shadow(float3 world_pos, float3 geom_N, float3 sun_dir, float bias)
{
    RayDesc ray;
    ray.Origin    = world_pos + geom_N * bias;
    ray.Direction = sun_dir;
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

float3 shade_surface_simple(float3 albedo, float3 N, float3 gN, float3 world_pos)
{
    float3 sun_dir = normalize(float3(0.6, 0.7, 0.3));
    float3 sun_color = float3(1.0, 0.95, 0.85) * 4.0;

    float bias = compute_bias(world_pos);
    float shadow = trace_shadow(world_pos, gN, sun_dir, bias);

    float NdotL = saturate(dot(N, sun_dir));
    float3 direct  = albedo * (NdotL / PI) * sun_color * shadow;
    float3 ambient = albedo * sample_sky(N) * 0.2;

    return ambient + direct;
}

BouncePayload trace_bounce(float3 origin, float3 geom_N, float3 sample_N, uint2 seed)
{
    float2 rand = hash2(seed);
    float3 bounce_dir = cosine_sample_hemisphere(sample_N, rand);

    RayDesc ray;
    ray.Origin    = origin + geom_N * compute_bias(origin);
    ray.Direction = bounce_dir;
    ray.TMin      = 0.001;
    ray.TMax      = 1000.0;

    BouncePayload payload = make_default_bounce_payload();

    TraceRay(
        tlas,
        0,
        RAY_MASK_ALL,
        BOUNCE_HIT_GROUP_OFFSET,
        BOUNCE_HIT_GROUP_STRIDE,
        BOUNCE_MISS_INDEX,
        ray,
        payload
    );

    return payload;
}

float3 bounce_light(BouncePayload payload)
{
    if (payload.hit)
        return shade_surface_simple(payload.hit_albedo, payload.N_shading, payload.N_geom, payload.world_pos);

    return payload.sky_color;
}

[shader("raygeneration")]
void raygen_main()
{
    uint2 pixel = DispatchRaysIndex().xy;
    if (pixel.x >= pc.width || pixel.y >= pc.height)
        return;

    float4 gbuf_n  = gbuf_normal.Load(int3(pixel, 0));
    float4 gbuf_a  = gbuf_albedo.Load(int3(pixel, 0));
    float2 gbuf_rm = gbuf_roughness_metal.Load(int3(pixel, 0));
    float  depth   = gbuf_depth.Load(int3(pixel, 0));

    float2 uv  = (float2(pixel) + 0.5) / float2(pc.width, pc.height);
    float2 ndc = uv * 2.0 - 1.0;

    if (depth <= 0.0)
    {
        float3 sky_hdr = sample_sky(reconstruct_world_ray_dir(ndc));
        color_out[pixel] = float4(linear_to_srgb(tonemap_aces(sky_hdr)), 1.0);
        return;
    }

    float3 N       = oct_decode(gbuf_n.xy);
    float3 gN      = oct_decode(gbuf_n.zw);
    float3 albedo  = gbuf_a.rgb;
    float3 hit_pos = reconstruct_world_pos(ndc, depth);

    // Roughness/metallic are loaded here so this pass is easy to extend back to
    // PBR later, but the simplified shader does not use them yet.

    float3 final_hdr = shade_surface_simple(albedo, N, gN, hit_pos);

    float3 indirect = 0.0;

    BouncePayload bp1 = trace_bounce(
        hit_pos,
        gN,
        N,
        pixel * uint2(1973u, 9277u) + uint2(0u, pc.frame_index)
    );

    float3 bounce1_light = bounce_light(bp1);

    indirect += bounce1_light * albedo * 0.15;

    if (bp1.hit)
    {
        BouncePayload bp2 = trace_bounce(
            bp1.world_pos,
            bp1.N_geom,
            bp1.N_shading,
            pixel * uint2(1973u, 9277u) + uint2(1u, pc.frame_index)
        );

        float3 bounce2_light = bounce_light(bp2);

        indirect += bounce2_light * bp1.hit_albedo * albedo * 0.03;
    }

    final_hdr += indirect;
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
    payload.sky_color = sample_sky(WorldRayDirection());
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

    float3 world_p0 = mul(di.model_matrix, float4(v0.position, 1.0)).xyz;
    float3 world_p1 = mul(di.model_matrix, float4(v1.position, 1.0)).xyz;
    float3 world_p2 = mul(di.model_matrix, float4(v2.position, 1.0)).xyz;
    float3 world_geom_N = normalize(cross(world_p1 - world_p0, world_p2 - world_p0));
    if (dot(world_geom_N, world_shading_N) < 0.0)
        world_geom_N = -world_geom_N;

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
    payload.N_geom     = world_geom_N;
    payload.roughness  = hit_roughness;
    payload.metallic   = hit_metallic;
    payload.sky_color  = float3(0, 0, 0);
}
