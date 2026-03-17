static const float DIFFUSE_IBL_LOD = 6.0;
static const float MAX_ENV_LOD = 8.0;

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

    return max(result, 0.0);
}