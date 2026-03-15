// rt.hlsl — Deferred lighting + shadow ray tracing pass.
// Reads GBuffer (normal, albedo, roughness/metallic, depth) written by the geometry pass.
// Traces 4 jittered shadow rays per pixel (soft sun) + 2-bounce diffuse GI.
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
    uint   frame_index; // used as RNG seed for cosine-weighted hemisphere sampling
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
[[vk::binding(0, 1)]] [[vk::image_format("bgra8")]]   RWTexture2D<float4> color_out;
[[vk::binding(1, 1)]] Texture2D<float4> gbuf_normal;          // encoded world normal (xyz*0.5+0.5, w unused)
[[vk::binding(2, 1)]] Texture2D<float4> gbuf_albedo;          // base color RGBA
[[vk::binding(3, 1)]] Texture2D<float2> gbuf_roughness_metal; // (roughness, metallic)
[[vk::binding(4, 1)]] Texture2D<float>  gbuf_depth;           // reverse-Z depth (1=near, 0=far)

// ---------------------------------------------------------------------------
// GGX / Cook-Torrance BRDF + IBL helpers
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

// Karis (UE4) split-sum BRDF approximation — no precomputed LUT required.
// Returns the scale and bias to F0: result = F0 * scale + bias.
float2 env_brdf_approx(float roughness, float NdotV)
{
    const float4 c0 = float4(-1.0, -0.0275, -0.572,  0.022);
    const float4 c1 = float4( 1.0,  0.0425,  1.040, -0.040);
    float4 r   = roughness * c0 + c1;
    float  a00 = min(r.x * r.x, exp2(-9.28 * NdotV)) * r.x + r.y;
    return float2(-1.04, 1.04) * a00 + r.zw;
}

// Specular IBL using the equirect skybox as a rough prefiltered env map.
// Higher roughness samples higher mip levels for blurrier reflections.
// Assumes skybox texture has ~9 mip levels (e.g., 2048px wide).
float3 ibl_specular(
    float3 N, float3 V, float3 albedo, float metallic, float roughness,
    int texIdx, int sampIdx, float exposure)
{
    float3 R = reflect(-V, N);
    float NdotV = max(dot(N, V), 1e-4);
    float env_lod = roughness * 8.0;
    float3 env = textures[texIdx].SampleLevel(samplers[sampIdx],
                     dir_to_equirect_uv(R), env_lod).rgb * exposure;
    float3 F0 = lerp(float3(0.04, 0.04, 0.04), albedo, metallic);
    float2 brdf = env_brdf_approx(roughness, NdotV);
    float3 F_ibl = F0 * brdf.x + brdf.y;
    return env * F_ibl;
}

// Sample skybox (HDR linear) in the given direction.
float3 sample_sky(float3 dir, int texIdx, int sampIdx, float exposure)
{
    return textures[texIdx].SampleLevel(samplers[sampIdx],
               dir_to_equirect_uv(dir), 0).rgb * exposure;
}

// Diffuse + specular ambient using EXR skybox.
// Diffuse: kD * albedo * irradiance from sky hemisphere sample.
// Specular: split-sum IBL using env_brdf_approx + mip-biased equirect lookup.
float3 ibl_ambient(
    float3 N, float3 V, float3 albedo,
    float metallic, float roughness,
    int texIdx, int sampIdx, float exposure, float ambient_floor)
{
    // kD for diffuse: reduced by metallic (metals have no diffuse).
    // Using 1 - metallic (not full Fresnel integral) keeps ambient diffuse
    // correct at grazing angles where full Fresnel would make walls/ceilings dark.
    float3 kD = float3(1.0 - metallic, 1.0 - metallic, 1.0 - metallic);

    // Sample EXR at the surface normal direction for diffuse irradiance.
    float3 sky_N = sample_sky(N, texIdx, sampIdx, exposure);

    // The sunset EXR lower hemisphere is near-black, so downward-facing
    // geometry (arches, ceilings) would be unlit without a floor.
    float3 bounce_light = float3(0.6, 0.48, 0.30) * exposure;
    float3 irradiance   = max(sky_N, bounce_light) * ambient_floor;

    // Specular IBL: mip-biased equirect lookup for reflection.
    float3 spec = ibl_specular(N, V, albedo, metallic, roughness,
                               texIdx, sampIdx, exposure);

    return kD * albedo * irradiance + spec;
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
// Hash-based RNG for cosine-weighted hemisphere sampling.
// seed = pixel_coord * scale + uint2(bounce_depth, frame_index)
// ---------------------------------------------------------------------------
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

// Cosine-weighted hemisphere sample around N using spherical coordinates.
// Returns a direction in the same hemisphere as N.
float3 cosine_sample_hemisphere(float3 N, float2 rand)
{
    // Malley's method: sample unit disk, then project up to hemisphere
    float r   = sqrt(rand.x);
    float phi = 2.0 * PI * rand.y;
    float x   = r * cos(phi);
    float y   = r * sin(phi);
    float z   = sqrt(max(0.0, 1.0 - rand.x));

    // Build orthonormal basis around N
    float3 up  = abs(N.y) < 0.999 ? float3(0, 1, 0) : float3(1, 0, 0);
    float3 T   = normalize(cross(up, N));
    float3 B   = cross(N, T);

    return normalize(x * T + y * B + z * N);
}

// ---------------------------------------------------------------------------
// Shadow ray payload — visibility flag only
// ---------------------------------------------------------------------------
struct ShadowPayload
{
    float visibility; // 1.0 = lit, 0.0 = shadowed
};

// ---------------------------------------------------------------------------
// Bounce payload — surface data returned by the bounce closest-hit shader
// ---------------------------------------------------------------------------
struct BouncePayload
{
    float3 hit_albedo;
    float3 world_pos;
    float3 N_shading;  // shading (normal-mapped) normal
    float3 N_geom;     // geometric/vertex normal (for ray bias)
    float  roughness;
    float  metallic;
    bool   hit;        // true if ray hit geometry, false if it missed (sky)
    float3 sky_color;  // sky radiance when hit == false
};

// ---------------------------------------------------------------------------
// Trace a shadow ray from world_pos toward sun_dir.
// Returns 1.0 if unoccluded, 0.0 if occluded.
// Uses geometric normal bias to avoid self-intersection.
// ---------------------------------------------------------------------------
float trace_shadow(float3 world_pos, float3 geom_N, float3 sun_dir, float bias)
{
    RayDesc shadow_ray;
    shadow_ray.Origin    = world_pos + geom_N * bias;
    shadow_ray.Direction = sun_dir;
    shadow_ray.TMin      = 0.005;
    shadow_ray.TMax      = 10000.0;

    ShadowPayload shadow;
    shadow.visibility = 0.0;

    TraceRay(
        tlas,
        RAY_FLAG_ACCEPT_FIRST_HIT_AND_END_SEARCH | RAY_FLAG_SKIP_CLOSEST_HIT_SHADER,
        0xFF,
        0, 1,
        0, // miss index 0 → shadow_miss_main
        shadow_ray,
        shadow
    );
    return shadow.visibility;
}

// ---------------------------------------------------------------------------
// Trace 4 jittered shadow rays toward a soft sun disc (half-angle ~0.5 deg).
// Averages visibility to produce a smooth soft shadow with reduced variance.
// seed_base: per-pixel seed mixed with sample index for independent samples.
// ---------------------------------------------------------------------------
float trace_shadow_soft(float3 world_pos, float3 geom_N, float3 sun_dir, float bias, uint2 seed_base)
{
    // Build a tangent frame around sun_dir for disc jitter
    float3 up_ref = abs(sun_dir.y) < 0.999 ? float3(0, 1, 0) : float3(1, 0, 0);
    float3 T = normalize(cross(up_ref, sun_dir));
    float3 B = cross(sun_dir, T);

    // Sun disc half-angle ~0.5 degrees in radians
    const float SUN_HALF_ANGLE = 0.0087266; // 0.5 * PI / 180

    float total = 0.0;
    [unroll]
    for (uint i = 0; i < 4; ++i)
    {
        uint2 seed = seed_base + uint2(i * 7919u, i * 6271u);
        float2 r   = hash2(seed);

        // Uniform disc sample within sun solid angle cone
        float radius = SUN_HALF_ANGLE * sqrt(r.x);
        float phi    = 2.0 * PI * r.y;
        float3 jitter_dir = normalize(sun_dir + (T * (radius * cos(phi)) + B * (radius * sin(phi))));

        total += trace_shadow(world_pos, geom_N, jitter_dir, bias);
    }
    return total * 0.25; // average of 4 samples
}

// ---------------------------------------------------------------------------
// Raygen: read GBuffer, reconstruct world pos from depth, shade + trace shadow
// + 2-bounce indirect diffuse GI
// ---------------------------------------------------------------------------
[shader("raygeneration")]
void raygen_main()
{
    uint2 pixel = DispatchRaysIndex().xy;
    if (pixel.x >= pc.width || pixel.y >= pc.height)
        return;

    const uint INVALID = 0xFFFFFFFFu;

    // GBuffer loads (integer, no interpolation)
    float4 gbuf_n  = gbuf_normal.Load(int3(pixel, 0));
    float4 gbuf_a  = gbuf_albedo.Load(int3(pixel, 0));
    float2 gbuf_rm = gbuf_roughness_metal.Load(int3(pixel, 0));
    float  depth   = gbuf_depth.Load(int3(pixel, 0));

    // NDC for this pixel
    float2 uv  = (float2(pixel) + 0.5) / float2(pc.width, pc.height);
    float2 ndc = uv * 2.0 - 1.0;

    // depth == 0 → reverse-Z far plane → sky pixel
    if (depth <= 0.0)
    {
        // Reconstruct view direction from NDC
        float4 near_w = mul(pc.inv_viewproj, float4(ndc, 1.0, 1.0));
        float4 far_w  = mul(pc.inv_viewproj, float4(ndc, 0.0, 1.0));
        near_w /= near_w.w;
        far_w  /= far_w.w;
        float3 ray_dir = normalize(far_w.xyz - near_w.xyz);

        float3 sky_hdr;
        if (skybox.tex != INVALID && skybox.sampler != INVALID)
        {
            int texIdx  = clamp((int)skybox.tex,    0, 1023);
            int sampIdx = clamp((int)skybox.sampler, 0, 1023);
            sky_hdr = textures[texIdx].SampleLevel(samplers[sampIdx],
                          dir_to_equirect_uv(ray_dir), 0).rgb * skybox.exposure;
        }
        else
        {
            // Fallback gradient sky
            float t = saturate(0.5 * (ray_dir.y + 1.0));
            sky_hdr = lerp(float3(0.3, 0.3, 0.35), float3(0.15, 0.35, 0.8), t);
        }

        color_out[pixel] = float4(linear_to_srgb(tonemap_aces(sky_hdr)), 1.0);
        return;
    }

    // Decode dual octahedral normals:
    //   xy → shading normal (normal-mapped, used for PBR)
    //   zw → geometric/vertex normal (used for shadow ray bias)
    float3 N  = oct_decode(gbuf_n.xy); // shading normal
    float3 gN = oct_decode(gbuf_n.zw); // geometric normal

    float3 albedo    = gbuf_a.rgb;
    float  roughness = max(gbuf_rm.x, 0.04);
    float  metallic  = gbuf_rm.y;

    // Reconstruct world-space position from reverse-Z depth
    float3 hit_pos = reconstruct_world_pos(ndc, depth, pc.inv_viewproj);
    float3 V       = normalize(pc.camera_pos.xyz - hit_pos);

    // Sun parameters
    float3 sun_dir   = normalize(float3(0.1, 1.0, 0.0));

    // -----------------------------------------------------------------------
    // Shadow ray bias (NdotL-scaled slope bias + view-distance scale).
    // Dividing by NdotL gives more offset when the surface is edge-on to the
    // sun — exactly when banner/fabric self-shadowing is worst.
    // The 0.5 cap prevents floating geometry at large view distances.
    // -----------------------------------------------------------------------
    float view_dist  = length(hit_pos - pc.camera_pos.xyz);
    float NdotL_sun  = max(dot(gN, sun_dir), 0.05);
    float bias       = min(max(0.02, view_dist * 0.001) / NdotL_sun, 0.5);
    float3 sun_color = float3(1.0, 0.95, 0.85) * 4.0;

    // 4-sample soft shadow (sun disc jitter): reduces shadow variance significantly
    uint2 shadow_seed = pixel * uint2(1973u, 9277u) + uint2(pc.frame_index * 3u, pc.frame_index * 7u);
    float shadow_visibility = trace_shadow_soft(hit_pos, gN, sun_dir, bias, shadow_seed);

    // -----------------------------------------------------------------------
    // Direct lighting (PBR)
    // -----------------------------------------------------------------------
    float3 direct = shadow_visibility
        * pbr_direct_light(N, V, sun_dir, albedo, metallic, roughness, sun_color);

    // -----------------------------------------------------------------------
    // Ambient / IBL — kept as linear HDR, tonemapped together with direct.
    // -----------------------------------------------------------------------
    float3 ambient = float3(0.03, 0.03, 0.03) * albedo; // fallback

    if (skybox.tex != INVALID && skybox.sampler != INVALID)
    {
        int texIdx  = clamp((int)skybox.tex,    0, 1023);
        int sampIdx = clamp((int)skybox.sampler, 0, 1023);
        ambient = ibl_ambient(N, V, albedo, metallic, roughness,
                              texIdx, sampIdx,
                              skybox.exposure, skybox.ambient_floor);
    }

    // -----------------------------------------------------------------------
    // Indirect GI — 2-bounce cosine-weighted diffuse
    // -----------------------------------------------------------------------
    // Fresnel at primary surface to compute kD for weighting indirect
    float3 F0_primary = lerp(float3(0.04, 0.04, 0.04), albedo, metallic);
    float3 F_primary  = F_Schlick(max(dot(N, V), 0.0), F0_primary);
    float3 kD_primary = (1.0 - F_primary) * (1.0 - metallic);

    float3 indirect = float3(0.0, 0.0, 0.0);

    // --- Bounce 1 ---
    {
        uint2 seed1 = pixel * uint2(1973u, 9277u) + uint2(0u, pc.frame_index);
        float2 rand1 = hash2(seed1);
        float3 bounce1_dir = cosine_sample_hemisphere(N, rand1);

        RayDesc b1_ray;
        b1_ray.Origin    = hit_pos + gN * bias;
        b1_ray.Direction = bounce1_dir;
        b1_ray.TMin      = 0.001;
        b1_ray.TMax      = 1000.0;

        BouncePayload bp1;
        bp1.hit        = false;
        bp1.hit_albedo = float3(0, 0, 0);
        bp1.world_pos  = float3(0, 0, 0);
        bp1.N_shading  = float3(0, 1, 0);
        bp1.N_geom     = float3(0, 1, 0);
        bp1.roughness  = 1.0;
        bp1.metallic   = 0.0;
        bp1.sky_color  = float3(0, 0, 0);

        TraceRay(
            tlas,
            0, // no skip flags — invoke bounce_hit_main
            0xFF,
            1, 1,   // sbtRecordOffset=1 → second hit group (bounce_hit_main); miss stride=1
            2,      // miss index 2 → bounce_miss_main
            b1_ray,
            bp1
        );

        float3 bounce1_color;
        if (bp1.hit)
        {
            // Direct lighting at the bounce-1 hit point
            float b1_view_dist = length(bp1.world_pos - pc.camera_pos.xyz);
            float NdotL_b1     = max(dot(bp1.N_geom, sun_dir), 0.05);
            float b1_bias      = min(max(0.02, b1_view_dist * 0.001) / NdotL_b1, 0.5);
            float b1_shadow    = trace_shadow(bp1.world_pos, bp1.N_geom, sun_dir, b1_bias);

            float3 b1_V = normalize(hit_pos - bp1.world_pos); // view from bounce point back toward primary
            float3 b1_direct = b1_shadow
                * pbr_direct_light(bp1.N_shading, b1_V, sun_dir,
                                   bp1.hit_albedo, bp1.metallic, bp1.roughness, sun_color);
            // pbr_direct_light already includes albedo; do not multiply again.
            bounce1_color = b1_direct;

            // --- Bounce 2 from bounce-1 hit ---
            float3 b2_color = float3(0.0, 0.0, 0.0);
            {
                uint2 seed2 = pixel * uint2(1973u, 9277u) + uint2(1u, pc.frame_index);
                float2 rand2 = hash2(seed2);
                float3 bounce2_dir = cosine_sample_hemisphere(bp1.N_shading, rand2);

                RayDesc b2_ray;
                b2_ray.Origin    = bp1.world_pos + bp1.N_geom * b1_bias;
                b2_ray.Direction = bounce2_dir;
                b2_ray.TMin      = 0.001;
                b2_ray.TMax      = 1000.0;

                BouncePayload bp2;
                bp2.hit        = false;
                bp2.hit_albedo = float3(0, 0, 0);
                bp2.world_pos  = float3(0, 0, 0);
                bp2.N_shading  = float3(0, 1, 0);
                bp2.N_geom     = float3(0, 1, 0);
                bp2.roughness  = 1.0;
                bp2.metallic   = 0.0;
                bp2.sky_color  = float3(0, 0, 0);

                TraceRay(
                    tlas,
                    0,
                    0xFF,
                    1, 1,   // sbtRecordOffset=1 → bounce_hit_main
                    2,      // miss index 2 → bounce_miss_main
                    b2_ray,
                    bp2
                );

                if (bp2.hit)
                {
                    float b2_view_dist = length(bp2.world_pos - pc.camera_pos.xyz);
                    float NdotL_b2     = max(dot(bp2.N_geom, sun_dir), 0.05);
                    float b2_bias      = min(max(0.02, b2_view_dist * 0.001) / NdotL_b2, 0.5);
                    float b2_shadow    = trace_shadow(bp2.world_pos, bp2.N_geom, sun_dir, b2_bias);

                    float3 b2_V = normalize(bp1.world_pos - bp2.world_pos);
                    float3 b2_direct = b2_shadow
                        * pbr_direct_light(bp2.N_shading, b2_V, sun_dir,
                                           bp2.hit_albedo, bp2.metallic, bp2.roughness, sun_color);
                    // pbr_direct_light already includes albedo; do not multiply again.
                    b2_color = b2_direct;
                }
                else
                {
                    b2_color = bp2.sky_color;
                }

                // kD at bounce-1 surface for weighting bounce-2 contribution
                float3 F0_b1 = lerp(float3(0.04, 0.04, 0.04), bp1.hit_albedo, bp1.metallic);
                float  cos_b1 = max(dot(bp1.N_shading, bounce2_dir), 0.0);
                float3 F_b1   = F_Schlick(cos_b1, F0_b1);
                float3 kD_b1  = (1.0 - F_b1) * (1.0 - bp1.metallic);

                // b2_color already contains albedo via pbr_direct_light; do not multiply by bp1.hit_albedo again.
                bounce1_color += b2_color * kD_b1 * 0.1;
            }
        }
        else
        {
            bounce1_color = bp1.sky_color;
        }

        // Accumulate bounce-1 into indirect.
        // cosine-weighted sampling cancels the cos/PI terms, so no extra factor needed.
        indirect += bounce1_color * kD_primary * albedo;
    }

    // -----------------------------------------------------------------------
    // Firefly suppression: clamp indirect contribution before accumulation.
    // Single bright GI samples (fireflies) are clamped to a reasonable maximum.
    // This sacrifices some energy at extreme highlights but is perceptually correct.
    // -----------------------------------------------------------------------
    indirect = min(indirect, float3(3.0, 3.0, 3.0));

    float3 final_hdr = ambient + direct + indirect;
    color_out[pixel] = float4(linear_to_srgb(tonemap_aces(final_hdr)), 1.0);
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
// Bounce miss — bounce ray hit sky: sample skybox color
// ---------------------------------------------------------------------------
[shader("miss")]
void bounce_miss_main(inout BouncePayload payload)
{
    payload.hit = false;

    float3 dir = WorldRayDirection();
    const uint INVALID = 0xFFFFFFFFu;
    if (skybox.tex != INVALID && skybox.sampler != INVALID)
    {
        int texIdx  = clamp((int)skybox.tex,    0, 1023);
        int sampIdx = clamp((int)skybox.sampler, 0, 1023);
        payload.sky_color = sample_sky(dir, texIdx, sampIdx, skybox.exposure);
    }
    else
    {
        float t = saturate(0.5 * (dir.y + 1.0));
        payload.sky_color = lerp(float3(0.3, 0.3, 0.35), float3(0.15, 0.35, 0.8), t);
    }
}

// ---------------------------------------------------------------------------
// Primary closest-hit (shadow ray dummy) — shadow rays always skip CHit via
// RAY_FLAG_SKIP_CLOSEST_HIT_SHADER. This entry exists so hit group 0 in the
// SBT is valid. It is never actually invoked.
// ---------------------------------------------------------------------------
[shader("closesthit")]
void primary_hit_main(inout ShadowPayload payload, BuiltInTriangleIntersectionAttributes attr)
{
    payload.visibility = 0.0;
}

// ---------------------------------------------------------------------------
// Bounce closest-hit — fetch vertex/material data and return surface info
// to the raygen shader via BouncePayload.
// ---------------------------------------------------------------------------
[shader("closesthit")]
void bounce_hit_main(inout BouncePayload payload, BuiltInTriangleIntersectionAttributes attr)
{
    uint instance_id = InstanceIndex();
    uint prim_id     = PrimitiveIndex();

    // Look up the DrawItem for this instance
    DrawItem di = dib[instance_id];

    uint vertex_offset = di.mesh_offsets.x;
    uint index_offset  = di.mesh_offsets.y;

    // Fetch triangle indices
    uint i0 = ib[index_offset + prim_id * 3 + 0];
    uint i1 = ib[index_offset + prim_id * 3 + 1];
    uint i2 = ib[index_offset + prim_id * 3 + 2];

    // Fetch vertices
    Vertex v0 = vb[vertex_offset + i0];
    Vertex v1 = vb[vertex_offset + i1];
    Vertex v2 = vb[vertex_offset + i2];

    // Barycentric interpolation
    float2 bary  = attr.barycentrics;
    float  w0    = 1.0 - bary.x - bary.y;
    float  w1    = bary.x;
    float  w2    = bary.y;

    // Interpolated UV
    float2 uv = v0.uv * w0 + v1.uv * w1 + v2.uv * w2;

    // Interpolated object-space normal → world space
    float3 local_N = normalize(v0.normal * w0 + v1.normal * w1 + v2.normal * w2);
    float3 world_N = normalize(mul((float3x3)di.normal_matrix, local_N));

    // World-space hit position
    float3 world_pos = WorldRayOrigin() + WorldRayDirection() * RayTCurrent();

    // Ray cone LOD: compute mip level from ray footprint at hit distance.
    // Cone half-angle approximates a 90-degree VFOV at 1080p resolution.
    float ray_t = RayTCurrent();
    const float CONE_HALF_ANGLE = 0.000926;
    float cone_width = CONE_HALF_ANGLE * ray_t;

    // Material lookup
    uint mat_idx = di.material.x;
    PbrMaterial mat = materials[mat_idx];

    // Albedo: sample texture or use base color
    float3 hit_albedo = mat.base_color.rgb;
    uint alb_tex = mat.albedo_tex.x;
    uint alb_smp = mat.albedo_sampler.x;
    if (alb_tex != 0xFFFFFFFFu && alb_smp != 0xFFFFFFFFu)
    {
        int ti = clamp((int)alb_tex, 0, 1023);
        int si = clamp((int)alb_smp, 0, 1023);
        float2 alb_dims;
        textures[ti].GetDimensions(alb_dims.x, alb_dims.y);
        float alb_lod = log2(max(cone_width * max(alb_dims.x, alb_dims.y), 1.0));
        hit_albedo *= textures[ti].SampleLevel(samplers[si], uv, alb_lod).rgb;
    }

    // Roughness / metallic: sample texture or use scalar
    float hit_roughness = max(mat.roughness, 0.04);
    float hit_metallic  = mat.metallness;
    uint rm_tex = mat.metallic_roughness_tex.x;
    uint rm_smp = mat.metallic_roughness_sampler.x;
    if (rm_tex != 0xFFFFFFFFu && rm_smp != 0xFFFFFFFFu)
    {
        int ti = clamp((int)rm_tex, 0, 1023);
        int si = clamp((int)rm_smp, 0, 1023);
        float2 rm_dims;
        textures[ti].GetDimensions(rm_dims.x, rm_dims.y);
        float rm_lod = log2(max(cone_width * max(rm_dims.x, rm_dims.y), 1.0));
        float2 rm = textures[ti].SampleLevel(samplers[si], uv, rm_lod).gb; // glTF: G=roughness, B=metallic
        hit_roughness = mat.roughness * rm.x;
        hit_metallic  = mat.metallness * rm.y;
    }

    payload.hit        = true;
    payload.hit_albedo = hit_albedo;
    payload.world_pos  = world_pos;
    payload.N_shading  = world_N; // no normal map at bounce — too expensive; use vertex normal
    payload.N_geom     = world_N;
    payload.roughness  = hit_roughness;
    payload.metallic   = hit_metallic;
    payload.sky_color  = float3(0, 0, 0);
}
