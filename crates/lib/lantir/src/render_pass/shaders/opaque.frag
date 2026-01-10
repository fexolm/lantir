#version 450

layout(location = 0) in vec4 vColor;
layout(location = 1) in vec2 vUv;
layout(location = 2) flat in uint vMaterialId;
layout(location = 0) out vec4 outColor;

// Matches `PbrMaterial` layout but avoids int64 requirements by treating `u64` as two `u32`.
struct PbrMaterial {
    uvec2 albedo_tex;
    uvec2 normal_tex;
    uvec2 metallic_roughness_tex;
    uvec2 emissive_tex;

    vec4 base_color;
    vec3 emissive_color;
    float metallness;
    float roughness;
};

layout(set = 0, binding = 1, std430) readonly buffer MaterialBuffer {
    PbrMaterial materials[];
} mb;

// Fixed-size array avoids SPIR-V RuntimeDescriptorArray capability.
layout(set = 0, binding = 2) uniform sampler2D textures[1024];

void main() {
    const uint INVALID = 0xFFFFFFFFu;

    vec4 base = vColor;
    if (vMaterialId != INVALID) {
        PbrMaterial mat = mb.materials[vMaterialId];
        base *= mat.base_color;

        uint albedo_id = mat.albedo_tex.x;
        if (albedo_id != INVALID) {
            int tex_index = clamp(int(albedo_id), 0, 1023);
            base *= texture(textures[tex_index], vUv);
        }
    }

    outColor = base;
}
