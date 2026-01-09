#version 450

#extension GL_GOOGLE_include_directive : require
#extension GL_EXT_buffer_reference : require

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

layout (location = 0) out vec3 outNormal;
layout (location = 1) out vec3 outColor;
layout (location = 2) out vec2 outUV;

struct Vertex {

	vec3 position;
	vec3 normal;
	vec4 color;
	vec2 uv;
};

layout(buffer_reference, std430) readonly buffer VertexBuffer{
	Vertex vertices[];
};

//push constants block
layout( push_constant ) uniform constants
{
	mat4 render_matrix;
	VertexBuffer vertexBuffer;
} PushConstants;

void main()
{
	Vertex v = PushConstants.vertexBuffer.vertices[gl_VertexIndex];

	vec4 position = vec4(v.position, 1.0f);

	gl_Position =  sceneData.viewproj * PushConstants.render_matrix *position;

	outNormal = (PushConstants.render_matrix * vec4(v.normal, 0.f)).xyz;
	outColor = v.color.xyz * materialData.colorFactors.xyz;
	outUV = v.uv;
}