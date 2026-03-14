use std::sync::Arc;

use anyhow::Context;

use lantir_hal::{
    AccessType, CommandBuffer, CopyImageInfo, DescriptorSet, DescriptorSetBinding,
    DescriptorSetLayout, ImageBarrier, PipelineLayout, RayTracingPipeline, RenderEngine, Shader,
    Texture, TextureCreateInfo, UpdateFrequency, WriteImageInfo, vk,
};

use crate::{
    include_shader,
    resources::resource_manager::ResourceManager,
    scene::Scene,
    world_renderer::WorldRenderer,
};

// ---------------------------------------------------------------------------
// Push constants (must match rt.hlsl)
// ---------------------------------------------------------------------------
#[repr(C)]
#[derive(Copy, Clone)]
struct RtPushConstants {
    inv_viewproj: glam::Mat4,
    camera_pos: glam::Vec4,
    width: u32,
    height: u32,
    _pad0: u32,
    _pad1: u32,
}

// ---------------------------------------------------------------------------
// RtPass
// ---------------------------------------------------------------------------
pub struct RtPass {
    pipeline: RayTracingPipeline,
    pipeline_layout: Arc<PipelineLayout>,
    // set=1: storage image output only. TLAS is in set=0 (meta DS, binding 7).
    descriptor_set: DescriptorSet,
    // Static output texture written by traceRays, then copied into the per-frame color_target.
    // Transitioned UNDEFINED→GENERAL once in new() via immediate_submit.
    rt_output: Texture,
    draw_extent: vk::Extent2D,
    color_target: Arc<Texture>,
}

impl RtPass {
    /// TLAS is owned by ResourceManager and bound via the meta descriptor set (set=0, binding=7).
    /// All static descriptors (storage image) are written here.
    pub fn new(
        engine: &Arc<RenderEngine>,
        resource_manager: &Arc<ResourceManager>,
        draw_extent: vk::Extent2D,
        color_target: Arc<Texture>,
    ) -> anyhow::Result<Self> {
        let shader = Shader::new_u32(engine.clone(), include_shader!("rt.hlsl"))
            .context("rt shader")?;

        // set=1: binding 0 = storage image (RT output).
        // TLAS lives in set=0 (meta descriptor set), binding META_BUFFER_BINDING_TLAS.
        let rt_ds_layout = DescriptorSetLayout::new(
            engine.clone(),
            &[DescriptorSetBinding {
                typ: vk::DescriptorType::STORAGE_IMAGE,
                binding: 0,
                stage: vk::ShaderStageFlags::RAYGEN_KHR,
                count: 1,
            }],
        )
        .context("RT DS layout")?;

        let push_range = [vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::RAYGEN_KHR
                | vk::ShaderStageFlags::MISS_KHR
                | vk::ShaderStageFlags::CLOSEST_HIT_KHR,
            offset: 0,
            size: std::mem::size_of::<RtPushConstants>() as u32,
        }];

        // set 0 = meta (scene buffers + TLAS), set 1 = RT-private (storage image)
        let pipeline_layout = PipelineLayout::new(
            engine.clone(),
            vec![
                resource_manager.get_meta_descriptor_set_layout().clone(),
                rt_ds_layout.clone(),
            ],
            &push_range,
        )
        .context("RT pipeline layout")?;

        let pipeline = RayTracingPipeline::new(engine.clone(), pipeline_layout.clone(), shader)
            .context("RT pipeline")?;

        let rt_output = Texture::new(
            engine.clone(),
            &TextureCreateInfo {
                image_type: vk::ImageType::TYPE_2D,
                update_frequency: UpdateFrequency::Static,
                format: vk::Format::B8G8R8A8_UNORM,
                extent: vk::Extent3D {
                    width: draw_extent.width,
                    height: draw_extent.height,
                    depth: 1,
                },
                usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
                aspect: vk::ImageAspectFlags::COLOR,
                mip_levels: 1,
            },
        )
        .context("rt_output texture")?;

        let descriptor_set =
            DescriptorSet::new(engine.clone(), rt_ds_layout).context("RT DS alloc")?;

        descriptor_set.write_image(&WriteImageInfo {
            binding: 0,
            image: &rt_output,
            layout: vk::ImageLayout::GENERAL,
            descriptor_type: vk::DescriptorType::STORAGE_IMAGE,
            sampler: None,
            array_index: 0,
        });

        // Transition rt_output UNDEFINED→GENERAL once so execute() always sees GENERAL.
        engine.immediate_submit(|cb| {
            cb.cmd_image_barrier(
                engine,
                &ImageBarrier {
                    previous_accesses: &[AccessType::Nothing],
                    next_accesses: &[AccessType::AnyShaderWrite],
                    previous_layout: vk::ImageLayout::UNDEFINED,
                    next_layout: vk::ImageLayout::GENERAL,
                    image: &rt_output,
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                },
            );
        })?;

        Ok(Self {
            pipeline,
            pipeline_layout,
            descriptor_set,
            rt_output,
            draw_extent,
            color_target,
        })
    }

    pub fn execute(
        &self,
        renderer: &WorldRenderer,
        scene: &Scene,
        cb: &CommandBuffer,
    ) -> anyhow::Result<()> {
        let engine = renderer.get_engine();
        let resource_manager = renderer.get_resource_manager();

        // WAW barrier: previous frame's traceRays write must complete before this frame's.
        cb.cmd_image_barrier(
            engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::AnyShaderWrite],
                next_accesses: &[AccessType::AnyShaderWrite],
                previous_layout: vk::ImageLayout::GENERAL,
                next_layout: vk::ImageLayout::GENERAL,
                image: &self.rt_output,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );

        cb.cmd_bind_rt_pipeline(engine, &self.pipeline);

        let meta_ds = resource_manager.get_meta_descriptor_set();
        cb.cmd_bind_descriptor_sets(
            engine,
            &self.pipeline_layout,
            &[meta_ds, &self.descriptor_set],
            vk::PipelineBindPoint::RAY_TRACING_KHR,
            0,
        );

        let push = RtPushConstants {
            inv_viewproj: scene.camera.inv_viewproj,
            camera_pos: scene.camera.camera_pos,
            width: self.draw_extent.width,
            height: self.draw_extent.height,
            _pad0: 0,
            _pad1: 0,
        };
        cb.cmd_push_constants(
            engine,
            &self.pipeline_layout,
            vk::ShaderStageFlags::RAYGEN_KHR
                | vk::ShaderStageFlags::MISS_KHR
                | vk::ShaderStageFlags::CLOSEST_HIT_KHR,
            0,
            &push,
        );

        cb.cmd_trace_rays(
            engine,
            &self.pipeline.raygen_region,
            &self.pipeline.miss_region,
            &self.pipeline.hit_region,
            &self.pipeline.callable_region,
            self.draw_extent.width,
            self.draw_extent.height,
        );

        // rt_output GENERAL → TRANSFER_SRC → copy into color_target → restore GENERAL
        cb.cmd_image_barrier(
            engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::AnyShaderWrite],
                next_accesses: &[AccessType::TransferRead],
                previous_layout: vk::ImageLayout::GENERAL,
                next_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image: &self.rt_output,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );
        cb.cmd_image_barrier(
            engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::ColorAttachmentWrite],
                next_accesses: &[AccessType::TransferWrite],
                previous_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                next_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                image: &*self.color_target,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );
        cb.cmd_copy_image(
            engine,
            &CopyImageInfo {
                src_image: &self.rt_output,
                src_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                src_aspect_mask: vk::ImageAspectFlags::COLOR,
                src_extent: self.draw_extent,
                dst_image: &*self.color_target,
                dst_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                dst_aspect_mask: vk::ImageAspectFlags::COLOR,
                dst_extent: self.draw_extent,
            },
        );
        cb.cmd_image_barrier(
            engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::TransferWrite],
                next_accesses: &[AccessType::ColorAttachmentWrite],
                previous_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                next_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                image: &*self.color_target,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );
        cb.cmd_image_barrier(
            engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::TransferRead],
                next_accesses: &[AccessType::AnyShaderWrite],
                previous_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                next_layout: vk::ImageLayout::GENERAL,
                image: &self.rt_output,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );

        Ok(())
    }
}
