struct Vertex
{
    float3 position;
    float3 normal;
    float4 color;
    float2 uv;
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
    float exposure;
    float ambient_floor;
};

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

float3 linear_to_srgb(float3 x)
{
    x = max(x, 0.0);
    float3 lo = x * 12.92;
    float3 hi = 1.055 * pow(x, 1.0 / 2.4) - 0.055;
    return lerp(hi, lo, step(x, 0.0031308));
}