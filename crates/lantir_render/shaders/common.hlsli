struct Vertex
{
    float3 position;
    float3 normal;
    float4 color;
    float2 uv;
    float4 tangent; // xyz = tangent direction in model space, w = bitangent sign (+1 or -1)
};

struct DrawItem
{
    column_major float4x4 model_matrix;
    column_major float4x4 normal_matrix;
    uint2 mesh;
    uint2 material;
    // mesh_offsets packed by ResourceManager::pack_mesh_offsets():
    //   .x = vertex_offset (first vertex in global VB for this mesh)
    //   .y = index_offset  (first index in global IB for this mesh)
    // Used by the RT hit shader to look up vertex attributes.
    uint2 mesh_offsets;
};

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
    uint1 blend_mode;
    float alpha_cutoff;
};

struct DynamicConstants
{
    column_major float4x4 viewproj;
    column_major float4x4 inv_viewproj;
    float4 camera_pos;
};

struct Skybox
{
    uint tex;
    uint sampler;

    uint irradiance_tex;
    uint irradiance_sampler;

    uint prefiltered_tex;
    uint prefiltered_sampler;

    uint brdf_lut_tex;
    uint brdf_lut_sampler;

    float exposure;
    float ambient_floor;
};

// ---------------------------------------------------------------------------
// Octahedral normal encoding — lossless round-trip for RGBA16F GBuffer.
// Maps unit vector ↔ float2 in [-1, 1]^2.
// ---------------------------------------------------------------------------
float2 oct_encode(float3 n)
{
    float l1 = abs(n.x) + abs(n.y) + abs(n.z);
    n /= l1;
    if (n.z < 0.0)
    {
        float ox = n.x;
        float oy = n.y;
        n.x = (1.0 - abs(oy)) * (ox >= 0.0 ? 1.0 : -1.0);
        n.y = (1.0 - abs(ox)) * (oy >= 0.0 ? 1.0 : -1.0);
    }
    return n.xy;
}

float3 oct_decode(float2 e)
{
    float3 v = float3(e.x, e.y, 1.0 - abs(e.x) - abs(e.y));
    if (v.z < 0.0)
    {
        float ox = v.x;
        v.x = (1.0 - abs(v.y)) * (ox >= 0.0 ? 1.0 : -1.0);
        v.y = (1.0 - abs(ox)) * (v.y >= 0.0 ? 1.0 : -1.0);
    }
    return normalize(v);
}

// ---------------------------------------------------------------------------
float2 dir_to_equirect_uv(float3 dir)
{
    dir = normalize(dir);
    float u = atan2(dir.z, dir.x) / (2.0 * 3.14159265359) + 0.5;
    float v = asin(clamp(dir.y, -1.0, 1.0)) / 3.14159265359 + 0.5;
    return float2(u, 1.0 - v);
}

float3 tonemap_reinhard(float3 x)
{
    return x / (1.0 + x);
}

float3 tonemap_aces(float3 x)
{
    const float a = 2.51, b = 0.03, c = 2.43, d = 0.59, e = 0.14;
    return saturate((x * (a * x + b)) / (x * (c * x + d) + e));
}

float3 linear_to_srgb(float3 x)
{
    x = max(x, 0.0);
    float3 lo = x * 12.92;
    float3 hi = 1.055 * pow(x, 1.0 / 2.4) - 0.055;
    return lerp(hi, lo, step(x, 0.0031308));
}