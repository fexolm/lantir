static const float DIFFUSE_IBL_LOD = 6.0;

float3 sample_sky(float3 dir, float lod)
{
    dir = normalize(dir);
    
    if (skybox.tex != INVALID && skybox.sampler != INVALID)
    {
        int texIdx  = clamp((int)skybox.tex, 0, 1023);
        int sampIdx = clamp((int)skybox.sampler, 0, 1023);
        return textures[texIdx].SampleLevel(
            samplers[sampIdx],
            dir_to_equirect_uv(dir),
            lod
        ).rgb * skybox.exposure;
    }

    float t = saturate(0.5 * (dir.y + 1.0));
    return lerp(float3(0.3, 0.3, 0.35), float3(0.15, 0.35, 0.8), t);
}

float2 dir_to_cube_face_uv(float3 dir, out uint face)
{
    float3 a = abs(dir);
    float u;
    float v;
    float ma;

    if (a.x >= a.y && a.x >= a.z)
    {
        ma = a.x;
        if (dir.x >= 0.0)
        {
            face = 0u; // px
            u = -dir.z;
            v = -dir.y;
        }
        else
        {
            face = 1u; // nx
            u = dir.z;
            v = -dir.y;
        }
    }
    else if (a.y >= a.x && a.y >= a.z)
    {
        ma = a.y;
        if (dir.y >= 0.0)
        {
            face = 2u; // py
            u = dir.x;
            v = dir.z;
        }
        else
        {
            face = 3u; // ny
            u = dir.x;
            v = -dir.z;
        }
    }
    else
    {
        ma = a.z;
        if (dir.z >= 0.0)
        {
            face = 4u; // pz
            u = dir.x;
            v = -dir.y;
        }
        else
        {
            face = 5u; // nz
            u = -dir.x;
            v = -dir.y;
        }
    }

    return 0.5 * (float2(u, v) / ma + 1.0);
}

uint prefiltered_mip_count(uint atlas_width, uint atlas_height)
{
    uint mip_count = 0u;
    uint face_size = max(atlas_width / 6u, 1u);
    uint consumed_height = 0u;

    while (face_size > 0u && consumed_height + face_size <= atlas_height)
    {
        mip_count += 1u;
        consumed_height += face_size;
        face_size = max(face_size >> 1u, 0u);
    }

    return max(mip_count, 1u);
}

float3 sample_prefiltered_env_level(float3 dir, uint mip_level)
{
    if (skybox.prefiltered_tex == INVALID || skybox.prefiltered_sampler == INVALID)
    {
        return sample_sky(dir, 0.0);
    }

    int texIdx  = clamp((int)skybox.prefiltered_tex, 0, 1023);
    int sampIdx = clamp((int)skybox.prefiltered_sampler, 0, 1023);

    uint atlas_width, atlas_height;
    textures[texIdx].GetDimensions(atlas_width, atlas_height);

    uint face_size = max(atlas_width / 6u, 1u);
    uint y_offset = 0u;
    for (uint i = 0u; i < mip_level; ++i)
    {
        y_offset += face_size;
        face_size = max(face_size >> 1u, 1u);
    }

    uint face = 0u;
    float2 face_uv = dir_to_cube_face_uv(normalize(dir), face);
    float2 atlas_px = float2(face * face_size, y_offset) + face_uv * (float(face_size) - 1.0);
    float2 atlas_uv = (atlas_px + 0.5) / float2(atlas_width, atlas_height);

    return textures[texIdx].SampleLevel(samplers[sampIdx], atlas_uv, 0.0).rgb * skybox.exposure;
}

float3 sample_prefiltered_env(float3 dir, float roughness)
{
    if (skybox.prefiltered_tex == INVALID || skybox.prefiltered_sampler == INVALID)
    {
        return sample_sky(dir, DIFFUSE_IBL_LOD);
    }

    int texIdx = clamp((int)skybox.prefiltered_tex, 0, 1023);
    uint atlas_width, atlas_height;
    textures[texIdx].GetDimensions(atlas_width, atlas_height);

    uint mip_count = prefiltered_mip_count(atlas_width, atlas_height);
    float mip = saturate(roughness) * float(max(mip_count - 1u, 0u));
    uint mip0 = (uint)floor(mip);
    uint mip1 = min(mip0 + 1u, mip_count - 1u);
    float t = frac(mip);

    float3 a = sample_prefiltered_env_level(dir, mip0);
    float3 b = sample_prefiltered_env_level(dir, mip1);
    return lerp(a, b, t);
}

float2 sample_brdf_lut(float ndotv, float roughness)
{
    if (skybox.brdf_lut_tex == INVALID || skybox.brdf_lut_sampler == INVALID)
    {
        return float2(1.0, 0.0);
    }

    int texIdx  = clamp((int)skybox.brdf_lut_tex, 0, 1023);
    int sampIdx = clamp((int)skybox.brdf_lut_sampler, 0, 1023);
    return textures[texIdx]
        .SampleLevel(samplers[sampIdx], float2(saturate(ndotv), saturate(roughness)), 0.0)
        .rg;
}

float3 reconstruct_world_ray_dir(float2 ndc_xy)
{
    float4 world = mul(pc.inv_viewproj, float4(ndc_xy, 0.0, 1.0));
    float3 world_pos = world.xyz / max(world.w, 1e-6);
    return normalize(world_pos - pc.camera_pos.xyz);
}

float2 pixel_to_uv(uint2 pixel)
{
    return (float2(pixel) + 0.5) / float2(pc.width, pc.height);
}

float2 pixel_to_ndc(uint2 pixel)
{
    float2 uv = pixel_to_uv(pixel);
    return uv * 2.0 - 1.0;
}

float load_depth_from_uv(float2 uv)
{
    if (uv.x < 0.0 || uv.x >= 1.0 || uv.y < 0.0 || uv.y >= 1.0)
        return 0.0;

    uint2 pixel = uint2(uv * float2(pc.width, pc.height));
    pixel = min(pixel, uint2(pc.width - 1u, pc.height - 1u));
    return gbuf_depth.Load(int3(pixel, 0));
}

float3 add_bias(float3 p, float3 n)
{
    int3 of_i = int3(INT_SCALE * n.x, INT_SCALE * n.y, INT_SCALE * n.z);

    float3 p_i = float3(
        asfloat(asint(p.x) + (p.x < 0.0 ? -of_i.x : of_i.x)),
        asfloat(asint(p.y) + (p.y < 0.0 ? -of_i.y : of_i.y)),
        asfloat(asint(p.z) + (p.z < 0.0 ? -of_i.z : of_i.z))
    );

    return float3(
        abs(p.x) < ORIGIN_THRESHOLD ? p.x + FLOAT_SCALE * n.x : p_i.x,
        abs(p.y) < ORIGIN_THRESHOLD ? p.y + FLOAT_SCALE * n.y : p_i.y,
        abs(p.z) < ORIGIN_THRESHOLD ? p.z + FLOAT_SCALE * n.z : p_i.z
    );
}

float3 reconstruct_world_pos_from_uv(float2 uv, float depth)
{
    // The camera projection already contains the Vulkan Y-flip, so we only
    // remap UV to NDC here and do not flip clipPos.y again.
    float4 clipPos = float4(uv * 2.0 - 1.0, depth, 1.0);
    float4 worldPos = mul(pc.inv_viewproj, clipPos);
    return worldPos.xyz / max(worldPos.w, 1e-6);
}

uint pcg_hash(uint seed) {
    uint state = seed * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

float rnd(inout uint seed) {
    seed = pcg_hash(seed);
    return float(seed) / 4294967296.0;
}

float3 add_jitter(float3 vec, float angular_radius, inout uint seed) {
    // 1. Создаем локальный базис вокруг направления солнца
    float3 up = abs(vec.z) < 0.999 ? float3(0, 0, 1) : float3(1, 0, 0);
    float3 tangent = normalize(cross(up, vec));
    float3 bitangent = cross(vec, tangent);

    // 2. Генерируем случайную точку на диске (равномерное распределение)
    float r = angular_radius * sqrt(rnd(seed));
    float phi = rnd(seed) * 2.0 * 3.14159265;

    float p_x = r * cos(phi);
    float p_y = r * sin(phi);

    // 3. Отклоняем основной луч
    return normalize(vec + p_x * tangent + p_y * bitangent);
}

float3 evaluate_sh9(float3 n, float4 sh[9])
{
    float x = n.x;
    float y = n.y;
    float z = n.z;

    float3 result =
        sh[0].rgb * 0.282095 +
        sh[1].rgb * (0.488603 * y) +
        sh[2].rgb * (0.488603 * z) +
        sh[3].rgb * (0.488603 * x) +
        sh[4].rgb * (1.092548 * x * y) +
        sh[5].rgb * (1.092548 * y * z) +
        sh[6].rgb * (0.315392 * (3.0 * z * z - 1.0)) +
        sh[7].rgb * (1.092548 * x * z) +
        sh[8].rgb * (0.546274 * (x * x - y * y));

    return max(result, 0.0) * skybox.exposure;
}
