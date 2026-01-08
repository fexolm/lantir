#version 450

// Shader input
layout (location = 0) in vec3 inNormal; // Входная нормаль

// Output write
layout (location = 0) out vec4 outFragColor;

void main()
{
    // Нормализуем нормаль
    vec3 normal = normalize(inNormal);

    // Освещение от нормалей
    vec3 lightDir = normalize(vec3(0.5, 1.0, 0.3)); // Направление света
    float lightIntensity = max(dot(normal, lightDir), 0.0);

    // Цвет на основе нормалей (для отладки)
    vec3 debugColor = normal * 0.5 + 0.5; // Преобразуем нормали в диапазон [0, 1]

    // Итоговый цвет: смешиваем отладочный цвет и освещение
    vec3 finalColor = mix(debugColor, vec3(0.5) * lightIntensity, 0.5);

    outFragColor = vec4(finalColor, 1.0);
}