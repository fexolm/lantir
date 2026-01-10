use lantir_hal::{
    BlendingMode, Buffer, CommandBuffer, DescriptorSet, DescriptorSetBinding, DescriptorSetLayout,
    GraphicsPipeline, GraphicsPipelineCreateInfo, PipelineLayout, RenderEngine, Sampler, Shader,
    Texture, WriteBufferInfo, WriteImageInfo, vk,
};
use shaderc::ShaderKind;
use std::sync::Arc;

pub struct MetallicRoughnessMat {
    pipeline: GraphicsPipeline,
    descriptor_set_layout: Arc<DescriptorSetLayout>,
}

pub struct MetallicRoughnessMatInstance {
    mat: Arc<MetallicRoughnessMat>,
    descriptor_set: DescriptorSet,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct GPUDrawPushConstants {
    pub render_matrix: glam::Mat4,
    pub vert_address: u64,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct MaterialConstants {
    pub color_factors: glam::Vec4,
    pub metal_rough_factors: glam::Vec4,
    pub extra: [glam::Vec4; 14],
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct SceneData {
    pub view: glam::Mat4,
    pub proj: glam::Mat4,
    pub viewproj: glam::Mat4,
    pub ambient_color: glam::Vec4,
    pub sunlignt_direction: glam::Vec4,
    pub sunlight_color: glam::Vec4,
}

impl MetallicRoughnessMat {
    pub fn new(engine: Arc<RenderEngine>) -> anyhow::Result<Arc<Self>> {
        let vertex_shader = load_shader(
            engine.clone(),
            "mesh.vert",
            include_str!("shaders/mesh.vert"),
            ShaderKind::Vertex,
        )?;
        let fragment_shader = load_shader(
            engine.clone(),
            "mesh.frag",
            include_str!("shaders/mesh.frag"),
            ShaderKind::Fragment,
        )?;

        let push_constants = [vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            offset: 0,
            size: std::mem::size_of::<GPUDrawPushConstants>() as u32,
        }];

        let scene_dsl = {
            let bindings = [DescriptorSetBinding {
                typ: vk::DescriptorType::UNIFORM_BUFFER,
                binding: 0,
                stage: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                count: 1,
            }];
            DescriptorSetLayout::new(engine.clone(), &bindings)?
        };

        let bindings = [
            DescriptorSetBinding {
                typ: vk::DescriptorType::UNIFORM_BUFFER,
                binding: 0,
                stage: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                count: 1,
            },
            DescriptorSetBinding {
                typ: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                binding: 1,
                stage: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                count: 1,
            },
            DescriptorSetBinding {
                typ: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                binding: 2,
                stage: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                count: 1,
            },
        ];

        let descriptor_set_layout = DescriptorSetLayout::new(engine.clone(), &bindings)?;

        let pipeline_layout = PipelineLayout::new(
            engine.clone(),
            vec![scene_dsl.clone(), descriptor_set_layout.clone()],
            &push_constants,
        )?;

        let pipeline = GraphicsPipeline::new(
            engine.clone(),
            &GraphicsPipelineCreateInfo {
                vertex_shader: &vertex_shader,
                fragment_shader: &fragment_shader,
                layout: &pipeline_layout,
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                polygon_mode: vk::PolygonMode::FILL,
                cull_mode: vk::CullModeFlags::NONE,
                front_face: vk::FrontFace::CLOCKWISE,
                color_attachment_format: vk::Format::B8G8R8A8_UNORM,
                depth_format: vk::Format::D32_SFLOAT,
                enable_depth_write: true,
                depth_compare_op: vk::CompareOp::LESS_OR_EQUAL,
                blending_mode: BlendingMode::AlphaBlend,
            },
        )?;

        Ok(Arc::new(Self {
            pipeline,
            descriptor_set_layout,
        }))
    }

    pub fn new_instance(
        self: &Arc<Self>,
        engine: Arc<RenderEngine>,
    ) -> anyhow::Result<MetallicRoughnessMatInstance> {
        let descriptor_set = DescriptorSet::new(engine, self.descriptor_set_layout.clone())?;

        Ok(MetallicRoughnessMatInstance {
            mat: self.clone(),
            descriptor_set,
        })
    }
}

impl MetallicRoughnessMatInstance {
    pub fn set_material_constants(&self, buf: &Buffer, offset: u64) {
        self.descriptor_set.write_buffer(&WriteBufferInfo {
            binding: 0,
            buffer: buf,
            size: size_of::<MaterialConstants>() as u64,
            offset: offset,
            descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
        });
    }

    pub fn set_color_image(&self, tex: &Texture, sampler: &Sampler) {
        self.descriptor_set.write_image(&WriteImageInfo {
            binding: 1,
            image: tex,
            layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            sampler: Some(sampler),
            array_index: 0,
        });
    }

    pub fn set_metal_rough_image(&self, tex: &Texture, sampler: &Sampler) {
        self.descriptor_set.write_image(&WriteImageInfo {
            binding: 2,
            image: tex,
            layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            sampler: Some(sampler),
            array_index: 0,
        });
    }

    pub fn push_constants(
        &self,
        engine: &RenderEngine,
        constants: &GPUDrawPushConstants,
        cb: &CommandBuffer,
    ) {
        cb.cmd_push_constants(
            &engine,
            &self.mat.pipeline.layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            constants,
        );
    }

    pub fn bind(&self, engine: &RenderEngine, cb: &CommandBuffer, scene_set: &DescriptorSet) {
        cb.cmd_bind_graphics_pipeline(engine, &self.mat.pipeline);
        cb.cmd_bind_descriptor_sets(
            engine,
            &self.mat.pipeline.layout,
            &[scene_set, &self.descriptor_set],
            vk::PipelineBindPoint::GRAPHICS,
            0,
        );
    }
}

fn load_shader(
    engine: Arc<RenderEngine>,
    name: &str,
    source: &str,
    shader_kind: ShaderKind,
) -> anyhow::Result<Arc<Shader>> {
    let compiler = shaderc::Compiler::new()?;
    let mut options = shaderc::CompileOptions::new()?;
    options.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_3 as u32,
    );
    options.set_target_spirv(shaderc::SpirvVersion::V1_5);
    options.set_generate_debug_info();

    let binary_result =
        compiler.compile_into_spirv(source, shader_kind, name, "main", Some(&options))?;

    assert_eq!(Some(&0x07230203), binary_result.as_binary().first());

    Shader::new(engine, binary_result.as_binary_u8())
}
