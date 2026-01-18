struct Vertex
{
    float3 position;
    float3 normal;
    float4 color;
    float2 uv;
};

struct DrawItem
{
    column_major float4x4 transform;
    uint2 mesh;
    uint2 material;
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