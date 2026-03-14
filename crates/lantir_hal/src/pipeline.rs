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
    /// Format of the first (or only) color attachment.
    pub color_attachment_format: vk::Format,
    /// Additional color attachment formats beyond the first one.
    /// When non-empty, the pipeline is created with 1 + extra.len() color attachments.
    pub extra_color_attachment_formats: &'i [vk::Format],
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

        // Build the full list of color attachment formats (first + extras).
        let mut color_attachment_formats: Vec<vk::Format> =
            Vec::with_capacity(1 + create_info.extra_color_attachment_formats.len());
        color_attachment_formats.push(create_info.color_attachment_format);
        color_attachment_formats.extend_from_slice(create_info.extra_color_attachment_formats);

        // One blend attachment state per color attachment, all sharing the same blending_mode.
        let single_blend = create_color_blend_attachment(&create_info.blending_mode);
        let color_blend_attachments: Vec<vk::PipelineColorBlendAttachmentState> =
            (0..color_attachment_formats.len())
                .map(|_| single_blend)
                .collect();

        let color_blending_state = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);

        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);

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

/// Description of one shader stage in a custom RT pipeline.
pub struct RtShaderStage<'a> {
    pub stage: vk::ShaderStageFlags,
    pub shader: &'a Arc<Shader>,
    /// Entry point name as a nul-terminated C string literal (e.g. `c"raygen_main"`).
    pub entry_point: &'a std::ffi::CStr,
}

/// Description of the SBT regions for a custom RT pipeline.
/// The caller is responsible for ensuring the counts match the groups passed to `new_custom`.
pub struct RtSbtDesc {
    /// Number of miss shader groups (consecutive in the groups slice after raygen).
    pub num_miss_groups: u32,
    /// Number of hit groups (consecutive in the groups slice after miss groups).
    pub num_hit_groups: u32,
}

impl RayTracingPipeline {
    /// Create a ray tracing pipeline with a fully custom set of shader stages and groups.
    ///
    /// `stages`: ordered list of (stage, shader, entry_point).
    /// `groups`: VkRayTracingShaderGroupCreateInfoKHR — must reference stage indices matching `stages`.
    ///   Layout convention: groups[0] = raygen, groups[1..1+num_miss] = miss, groups[1+num_miss..] = hit.
    /// `sbt_desc`: counts for building the SBT.
    /// `max_recursion_depth`: passed to VkRayTracingPipelineCreateInfoKHR.
    pub fn new_custom(
        engine: Arc<RenderEngine>,
        layout: Arc<PipelineLayout>,
        stages: &[RtShaderStage<'_>],
        groups: &[vk::RayTracingShaderGroupCreateInfoKHR<'_>],
        sbt_desc: RtSbtDesc,
        max_recursion_depth: u32,
    ) -> anyhow::Result<Self> {
        let shaders: Vec<Arc<Shader>> = stages.iter().map(|s| s.shader.clone()).collect();
        let data = RayTracingPipelineData::new_custom(
            &engine,
            layout,
            shaders,
            stages,
            groups,
            sbt_desc,
            max_recursion_depth,
        )?;
        Ok(Resource::make(engine, data))
    }
}

fn align_up_u64(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

impl RayTracingPipelineData {
    /// Build a pipeline from an arbitrary set of shader stages and groups.
    ///
    /// Groups layout convention (matching SBT layout built here):
    ///   groups[0]                            = raygen group (always exactly 1)
    ///   groups[1 .. 1+num_miss]              = miss groups
    ///   groups[1+num_miss .. 1+num_miss+num_hit] = hit groups
    pub fn new_custom(
        engine: &Arc<RenderEngine>,
        layout: Arc<PipelineLayout>,
        shaders: Vec<Arc<Shader>>,
        stages: &[RtShaderStage<'_>],
        groups: &[vk::RayTracingShaderGroupCreateInfoKHR<'_>],
        sbt_desc: RtSbtDesc,
        max_recursion_depth: u32,
    ) -> anyhow::Result<Self> {
        let loader = &engine.ray_tracing_pipeline_loader;

        let vk_stages: Vec<vk::PipelineShaderStageCreateInfo<'_>> = stages
            .iter()
            .map(|s| {
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(s.stage)
                    .module(s.shader.shader)
                    .name(s.entry_point)
            })
            .collect();

        let pipeline_info = vk::RayTracingPipelineCreateInfoKHR::default()
            .stages(&vk_stages)
            .groups(groups)
            .max_pipeline_ray_recursion_depth(max_recursion_depth)
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

        let rt_props = engine.ray_tracing_pipeline_properties();
        let handle_size = rt_props.shader_group_handle_size as u64;
        let handle_alignment = rt_props.shader_group_handle_alignment as u64;
        let base_alignment = rt_props.shader_group_base_alignment as u64;

        let entry_stride = align_up_u64(handle_size, handle_alignment);

        let num_groups = groups.len() as u32;
        let num_miss = sbt_desc.num_miss_groups;
        let num_hit = sbt_desc.num_hit_groups;

        // Raygen region: exactly 1 entry
        let raygen_size = entry_stride;
        // Miss region: num_miss entries
        let miss_size = align_up_u64(entry_stride * num_miss as u64, base_alignment);
        // Hit region: num_hit entries
        let hit_size = align_up_u64(entry_stride * num_hit as u64, base_alignment);

        let raygen_offset: u64 = 0;
        let miss_offset = align_up_u64(raygen_offset + raygen_size, base_alignment);
        let hit_offset = align_up_u64(miss_offset + miss_size, base_alignment);
        let total_size = hit_offset + hit_size;

        let handle_data_size = (handle_size * num_groups as u64) as usize;
        let handles: Vec<u8> = unsafe {
            loader.get_ray_tracing_shader_group_handles(pipeline, 0, num_groups, handle_data_size)?
        };

        let hs = handle_size as usize;
        let es = entry_stride as usize;
        let mut sbt_host = vec![0u8; total_size as usize];

        // Raygen handle (group 0)
        sbt_host[raygen_offset as usize..raygen_offset as usize + hs]
            .copy_from_slice(&handles[0..hs]);

        // Miss handles (groups 1..1+num_miss)
        for i in 0..num_miss as usize {
            let src_start = (1 + i) * hs;
            let dst_start = miss_offset as usize + i * es;
            sbt_host[dst_start..dst_start + hs]
                .copy_from_slice(&handles[src_start..src_start + hs]);
        }

        // Hit handles (groups 1+num_miss .. 1+num_miss+num_hit)
        for i in 0..num_hit as usize {
            let src_start = (1 + num_miss as usize + i) * hs;
            let dst_start = hit_offset as usize + i * es;
            sbt_host[dst_start..dst_start + hs]
                .copy_from_slice(&handles[src_start..src_start + hs]);
        }

        let staging = Buffer::new(
            engine.clone(),
            &BufferCreateInfo {
                size: total_size,
                usage: vk::BufferUsageFlags::TRANSFER_SRC,
                memory_property: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
                update_frequency: UpdateFrequency::Static,
                vma_flags: AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            },
        )?;
        unsafe {
            let ptr = staging.map()?;
            std::ptr::copy_nonoverlapping(sbt_host.as_ptr(), ptr, total_size as usize);
            staging.unmap();
        }

        let sbt_buffer = Buffer::new(
            engine.clone(),
            &BufferCreateInfo {
                size: total_size,
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
                    size: total_size,
                },
            );
        })?;

        let base = sbt_buffer.get_device_address();

        let raygen_region = vk::StridedDeviceAddressRegionKHR {
            device_address: base + raygen_offset,
            stride: raygen_size,
            size: raygen_size,
        };
        let miss_region = vk::StridedDeviceAddressRegionKHR {
            device_address: base + miss_offset,
            stride: entry_stride,
            size: miss_size,
        };
        let hit_region = vk::StridedDeviceAddressRegionKHR {
            device_address: base + hit_offset,
            stride: entry_stride,
            size: hit_size,
        };
        let callable_region = vk::StridedDeviceAddressRegionKHR::default();

        Ok(Self {
            pipeline,
            layout,
            _shaders: shaders,
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
