use crate::buffer::{Buffer, BufferCreateInfo};
use crate::{RenderEngine, UpdateFrequency};
use crate::descriptor_set::DescriptorSetLayout;
use crate::resource::{DeferDrop, Resource};
use crate::shader::Shader;
use ash::vk;
use std::sync::Arc;
use vk_mem::AllocationCreateFlags;

pub type PipelineLayout = Resource<PipelineLayoutData>;

impl PipelineLayout {
    pub fn new(
        engine: Arc<RenderEngine>,
        descriptor_sets: Vec<Arc<DescriptorSetLayout>>,
        push_constants: &[vk::PushConstantRange],
    ) -> anyhow::Result<Arc<Self>> {
        let data = PipelineLayoutData::new(&engine, descriptor_sets, push_constants)?;
        Ok(Arc::new(Resource::make(engine, data)))
    }
}

pub struct PipelineLayoutData {
    pub(crate) layout: vk::PipelineLayout,

    _descriptor_sets: Vec<Arc<DescriptorSetLayout>>,
}

impl PipelineLayoutData {
    pub fn new(
        engine: &RenderEngine,
        descriptor_sets: Vec<Arc<DescriptorSetLayout>>,
        push_constants: &[vk::PushConstantRange],
    ) -> anyhow::Result<Self> {
        let layouts = descriptor_sets.iter().map(|s| s.layout).collect::<Vec<_>>();

        let info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&layouts)
            .push_constant_ranges(push_constants);

        let layout = unsafe { engine.device.create_pipeline_layout(&info, None)? };

        Ok(Self {
            layout,
            _descriptor_sets: descriptor_sets,
        })
    }
}

impl DeferDrop for PipelineLayoutData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            engine.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

pub type ComputePipeline = Resource<ComputePipelineData>;

impl ComputePipeline {
    pub fn new(
        engine: Arc<RenderEngine>,
        layout: Arc<PipelineLayout>,
        shader: Arc<Shader>,
    ) -> anyhow::Result<Self> {
        let data = ComputePipelineData::new(&engine, layout, shader)?;
        Ok(Resource::make(engine, data))
    }
}

pub struct ComputePipelineData {
    pub(crate) pipeline: vk::Pipeline,

    pub layout: Arc<PipelineLayout>,

    _shader: Arc<Shader>,
}

impl ComputePipelineData {
    pub fn new(
        engine: &RenderEngine,
        layout: Arc<PipelineLayout>,
        shader: Arc<Shader>,
    ) -> anyhow::Result<Self> {
        let stage_info = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(shader.shader)
            .name(c"cs_main");

        let infos = [vk::ComputePipelineCreateInfo::default()
            .stage(stage_info)
            .layout(layout.layout)];

        let pipeline = unsafe {
            engine
                .device
                .create_compute_pipelines(vk::PipelineCache::null(), &infos, None)
                .map_err(|(_, e)| e)?[0]
        };

        Ok(Self {
            pipeline,
            layout,
            _shader: shader,
        })
    }
}

impl DeferDrop for ComputePipelineData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            engine.device.destroy_pipeline(self.pipeline, None);
        }
    }
}

pub enum BlendingMode {
    AlphaBlend,
    Additive,
    NoBlend,
}

pub struct Specialization<'i> {
    pub map_entries: &'i [vk::SpecializationMapEntry],
    pub data: &'i [u8],
}

pub struct GraphicsPipelineCreateInfo<'i> {
    pub vertex_shader: &'i Arc<Shader>,
    pub fragment_shader: &'i Arc<Shader>,
    pub layout: &'i Arc<PipelineLayout>,
    pub vertex_specialization: Option<Specialization<'i>>,
    pub fragment_specialization: Option<Specialization<'i>>,
    pub topology: vk::PrimitiveTopology,
    pub polygon_mode: vk::PolygonMode,
    pub cull_mode: vk::CullModeFlags,
    pub front_face: vk::FrontFace,
    pub color_attachment_format: vk::Format,
    pub depth_format: vk::Format,
    pub enable_depth_write: bool,
    pub depth_compare_op: vk::CompareOp,
    pub blending_mode: BlendingMode,
}

pub type GraphicsPipeline = Resource<GraphicsPipelineData>;

impl GraphicsPipeline {
    pub fn new(
        engine: Arc<RenderEngine>,
        create_info: &GraphicsPipelineCreateInfo,
    ) -> anyhow::Result<Self> {
        let data = GraphicsPipelineData::new(&engine, create_info)?;
        Ok(Resource::make(engine.clone(), data))
    }
}

pub struct GraphicsPipelineData {
    pub(crate) pipeline: vk::Pipeline,

    pub layout: Arc<PipelineLayout>,
    _shaders: [Arc<Shader>; 2],
}

fn create_color_blend_attachment(mode: &BlendingMode) -> vk::PipelineColorBlendAttachmentState {
    let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD);

    match mode {
        BlendingMode::AlphaBlend => blend_attachment
            .blend_enable(true)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA),

        BlendingMode::Additive => blend_attachment
            .blend_enable(true)
            .dst_color_blend_factor(vk::BlendFactor::ONE),
        BlendingMode::NoBlend => blend_attachment.blend_enable(false),
    }
}

impl GraphicsPipelineData {
    pub fn new(
        engine: &RenderEngine,
        create_info: &GraphicsPipelineCreateInfo,
    ) -> anyhow::Result<Self> {
        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default();

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(create_info.topology)
            .primitive_restart_enable(false);

        let tessellation_state = vk::PipelineTessellationStateCreateInfo::default();

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(create_info.polygon_mode)
            .line_width(1.)
            .cull_mode(create_info.cull_mode)
            .front_face(create_info.front_face);

        let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(create_info.enable_depth_write)
            .depth_compare_op(create_info.depth_compare_op)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false)
            .min_depth_bounds(0.)
            .max_depth_bounds(1.);

        let color_blend_attachments = [create_color_blend_attachment(&create_info.blending_mode)];

        let color_blending_state = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);

        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);

        let color_attachment_formats = [create_info.color_attachment_format];

        let vs_spec_info = create_info.vertex_specialization.as_ref().map(|spec| {
            vk::SpecializationInfo::default()
                .map_entries(spec.map_entries)
                .data(spec.data)
        });
        let fs_spec_info = create_info.fragment_specialization.as_ref().map(|spec| {
            vk::SpecializationInfo::default()
                .map_entries(spec.map_entries)
                .data(spec.data)
        });

        let mut vs_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(create_info.vertex_shader.shader)
            .name(c"vs_main");

        if let Some(spec) = &vs_spec_info {
            vs_stage = vs_stage.specialization_info(spec);
        }

        let mut fs_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(create_info.fragment_shader.shader)
            .name(c"ps_main");

        if let Some(spec) = &fs_spec_info {
            fs_stage = fs_stage.specialization_info(spec);
        }

        let shader_stages = [vs_stage, fs_stage];

        let mut render_info = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&color_attachment_formats)
            .depth_attachment_format(create_info.depth_format);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .push_next(&mut render_info)
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_state)
            .tessellation_state(&tessellation_state)
            .input_assembly_state(&input_assembly_state)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization_state)
            .multisample_state(&multisample_state)
            .color_blend_state(&color_blending_state)
            .depth_stencil_state(&depth_stencil_state)
            .layout(create_info.layout.layout)
            .dynamic_state(&dynamic_state);

        let pipeline = unsafe {
            engine
                .device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
                .map_err(|(_, e)| anyhow::anyhow!("Failed to create graphics pipeline: {e:?}"))
                .map(|v| v[0])
        }?;

        Ok(GraphicsPipelineData {
            pipeline,
            layout: create_info.layout.clone(),
            _shaders: [
                create_info.vertex_shader.clone(),
                create_info.fragment_shader.clone(),
            ],
        })
    }
}

impl DeferDrop for GraphicsPipelineData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            engine.device.destroy_pipeline(self.pipeline, None);
        }
    }
}

// ---------------------------------------------------------------------------
// RayTracingPipeline
// ---------------------------------------------------------------------------

/// Shader group indices within the ray tracing pipeline.
const RAYGEN_GROUP_INDEX: u32 = 0;
const MISS_GROUP_INDEX: u32 = 1;
const HIT_GROUP_INDEX: u32 = 2;

pub type RayTracingPipeline = Resource<RayTracingPipelineData>;

pub struct RayTracingPipelineData {
    pub pipeline: vk::Pipeline,
    pub layout: Arc<PipelineLayout>,
    _shaders: Vec<Arc<Shader>>,
    /// Buffer holding the packed SBT (raygen | miss | hit).
    _sbt_buffer: Buffer,
    pub raygen_region: vk::StridedDeviceAddressRegionKHR,
    pub miss_region: vk::StridedDeviceAddressRegionKHR,
    pub hit_region: vk::StridedDeviceAddressRegionKHR,
    pub callable_region: vk::StridedDeviceAddressRegionKHR,
}

impl RayTracingPipeline {
    /// Create a ray tracing pipeline from a single SPIR-V shader module (rt.hlsl) that contains
    /// all three entry points: `raygen_main`, `miss_main`, and `hit_main`.
    pub fn new(
        engine: Arc<RenderEngine>,
        layout: Arc<PipelineLayout>,
        shader: Arc<Shader>,
    ) -> anyhow::Result<Self> {
        let data = RayTracingPipelineData::new(&engine, layout, shader)?;
        Ok(Resource::make(engine, data))
    }
}

fn align_up_u64(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

impl RayTracingPipelineData {
    pub fn new(
        engine: &Arc<RenderEngine>,
        layout: Arc<PipelineLayout>,
        shader: Arc<Shader>,
    ) -> anyhow::Result<Self> {
        let loader = &engine.ray_tracing_pipeline_loader;

        // All three entry points come from the same SPIR-V module (rt.hlsl).
        // Shader stages: raygen(0), miss(1), closesthit(2)
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::RAYGEN_KHR)
                .module(shader.shader)
                .name(c"raygen_main"),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::MISS_KHR)
                .module(shader.shader)
                .name(c"miss_main"),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::CLOSEST_HIT_KHR)
                .module(shader.shader)
                .name(c"hit_main"),
        ];

        // Shader groups: raygen, miss, hit
        let groups = [
            // Raygen group (index 0)
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(RAYGEN_GROUP_INDEX)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
            // Miss group (index 1)
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(MISS_GROUP_INDEX)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
            // Hit group (index 2): triangle hit group
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
                .general_shader(vk::SHADER_UNUSED_KHR)
                .closest_hit_shader(HIT_GROUP_INDEX)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
        ];

        let pipeline_info = vk::RayTracingPipelineCreateInfoKHR::default()
            .stages(&stages)
            .groups(&groups)
            .max_pipeline_ray_recursion_depth(1)
            .layout(layout.layout);

        let pipelines = unsafe {
            loader
                .create_ray_tracing_pipelines(
                    vk::DeferredOperationKHR::null(),
                    vk::PipelineCache::null(),
                    &[pipeline_info],
                    None,
                )
                .map_err(|(_, e)| anyhow::anyhow!("create_ray_tracing_pipelines failed: {e:?}"))?
        };
        let pipeline = pipelines[0];

        // Query RT pipeline properties for SBT alignment.
        let rt_props = engine.ray_tracing_pipeline_properties();
        let handle_size = rt_props.shader_group_handle_size as u64;
        let handle_alignment = rt_props.shader_group_handle_alignment as u64;
        let base_alignment = rt_props.shader_group_base_alignment as u64;

        // Each SBT entry stride is handle_size rounded up to handle_alignment.
        let entry_stride = align_up_u64(handle_size, handle_alignment);

        // Raygen region: size == stride (Vulkan spec requirement for raygen).
        // Miss and hit regions start at base_alignment boundaries.
        let raygen_region_size = entry_stride;
        let region_size = align_up_u64(entry_stride, base_alignment);
        let miss_region_size = region_size;
        let hit_region_size = region_size;

        // Raygen must start at a base_alignment boundary (offset 0 is aligned).
        // Miss starts after raygen, aligned to base_alignment.
        let raygen_offset: u64 = 0;
        let miss_offset = align_up_u64(raygen_offset + raygen_region_size, base_alignment);
        let hit_offset = align_up_u64(miss_offset + miss_region_size, base_alignment);
        let total_sbt_size = hit_offset + hit_region_size;

        // Fetch handles from the driver.
        let num_groups = groups.len() as u32;
        let handle_data_size = (handle_size as u32 * num_groups) as usize;
        let handles: Vec<u8> = unsafe {
            loader.get_ray_tracing_shader_group_handles(
                pipeline,
                0,
                num_groups,
                handle_data_size,
            )?
        };

        // Build SBT host buffer.
        let mut sbt_host = vec![0u8; total_sbt_size as usize];

        // Copy raygen handle at raygen_offset
        sbt_host[raygen_offset as usize..raygen_offset as usize + handle_size as usize]
            .copy_from_slice(&handles[0..handle_size as usize]);

        // Copy miss handle at miss_offset
        sbt_host[miss_offset as usize..miss_offset as usize + handle_size as usize]
            .copy_from_slice(&handles[handle_size as usize..2 * handle_size as usize]);

        // Copy hit handle at hit_offset
        sbt_host[hit_offset as usize..hit_offset as usize + handle_size as usize]
            .copy_from_slice(&handles[2 * handle_size as usize..3 * handle_size as usize]);

        // Upload SBT to device-local buffer.
        let staging = Buffer::new(
            engine.clone(),
            &BufferCreateInfo {
                size: total_sbt_size,
                usage: vk::BufferUsageFlags::TRANSFER_SRC,
                memory_property: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
                update_frequency: UpdateFrequency::Static,
                vma_flags: AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            },
        )?;
        unsafe {
            let ptr = staging.map()?;
            std::ptr::copy_nonoverlapping(sbt_host.as_ptr(), ptr, total_sbt_size as usize);
            staging.unmap();
        }

        let sbt_buffer = Buffer::new(
            engine.clone(),
            &BufferCreateInfo {
                size: total_sbt_size,
                usage: vk::BufferUsageFlags::SHADER_BINDING_TABLE_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::TRANSFER_DST,
                memory_property: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                update_frequency: UpdateFrequency::Static,
                vma_flags: AllocationCreateFlags::empty(),
            },
        )?;

        engine.immediate_submit(|cb| {
            cb.cmd_copy_buffer(
                engine,
                &staging,
                &sbt_buffer,
                vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: total_sbt_size,
                },
            );
        })?;

        let sbt_base = sbt_buffer.get_device_address();

        let raygen_region = vk::StridedDeviceAddressRegionKHR {
            device_address: sbt_base + raygen_offset,
            stride: raygen_region_size, // For raygen, stride == size
            size: raygen_region_size,
        };
        let miss_region = vk::StridedDeviceAddressRegionKHR {
            device_address: sbt_base + miss_offset,
            stride: entry_stride,
            size: miss_region_size,
        };
        let hit_region = vk::StridedDeviceAddressRegionKHR {
            device_address: sbt_base + hit_offset,
            stride: entry_stride,
            size: hit_region_size,
        };
        let callable_region = vk::StridedDeviceAddressRegionKHR::default();

        Ok(Self {
            pipeline,
            layout,
            _shaders: vec![shader],
            _sbt_buffer: sbt_buffer,
            raygen_region,
            miss_region,
            hit_region,
            callable_region,
        })
    }
}

impl DeferDrop for RayTracingPipelineData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            engine.device.destroy_pipeline(self.pipeline, None);
        }
    }
}
