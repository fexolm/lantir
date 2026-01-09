#version 450

#extension GL_GOOGLE_include_directive : require
layout(set = 0, binding = 0) uniform  SceneData{

	mat4 view;
	mat4 proj;
	mat4 viewproj;
	vec4 ambientColor;
	vec4 sunlightDirection; //w for sun power
	vec4 sunlightColor;
} sceneData;

layout(set = 1, binding = 0) uniform GLTFMaterialData{

	vec4 colorFactors;
	vec4 metal_rough_factors;

} materialData;

layout(set = 1, binding = 1) uniform sampler2D colorTex;
layout(set = 1, binding = 2) uniform sampler2D metalRoughTex;

layout (location = 0) in vec3 inNormal;
layout (location = 1) in vec3 inColor;
layout (location = 2) in vec2 inUV;

layout (location = 0) out vec4 outFragColor;

void main()
{
    // 1. Берем только текстуру (так как в вершинах цвета нет)
    vec3 color = texture(colorTex, inUV).xyz;

    // 2. Считаем освещение
    // Обязательно нормализуйте inNormal, она могла "поплыть" при интерполяции
    float lightValue = max(dot(normalize(inNormal), sceneData.sunlightDirection.xyz), 0.0f);

    // 3. Смешиваем (Sunlight + Ambient)
    // sunlightColor.w должен быть интенсивностью (например, 1.0)
    vec3 diffuse = color * lightValue * sceneData.sunlightColor.xyz * sceneData.sunlightColor.w;
    vec3 ambient = color * sceneData.ambientColor.xyz;

    outFragColor = vec4(diffuse + ambient, 1.0f);
}