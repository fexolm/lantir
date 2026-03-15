use std::sync::Arc;

use anyhow::Context;

use lantir_hal::{
    AccessType, CommandBuffer, CopyImageInfo, DescriptorSet, DescriptorSetBinding,
    DescriptorSetLayout, ImageBarrier, PipelineLayout, RayTracingPipeline, RenderEngine, RtSbtDesc,
    RtShaderStage, Shader, Texture, TextureCreateInfo, UpdateFrequency, WriteImageInfo, vk,
};

use crate::{
    include_shader, resources::resource_manager::ResourceManager, scene::Scene,
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
    frame_index: u32,
}

// ---------------------------------------------------------------------------
// RtPass
// ---------------------------------------------------------------------------
pub struct RtPass {
    pipeline: RayTracingPipeline,
    pipeline_layout: Arc<PipelineLayout>,
    /// set=1: storage image output + GBuffer sampled images + depth sampled image
    descriptor_set: DescriptorSet,
    /// RT output storage image (written by traceRays, then blitted to color_target)
    rt_output: Texture,
    draw_extent: vk::Extent2D,
    color_target: Arc<Texture>,
}

impl RtPass {
    pub fn new(
        engine: &Arc<RenderEngine>,
        resource_manager: &Arc<ResourceManager>,
        draw_extent: vk::Extent2D,
        color_target: Arc<Texture>,
        gbuf_normal: Arc<Texture>,
        gbuf_albedo: Arc<Texture>,
        gbuf_roughness_metal: Arc<Texture>,
        depth_target: Arc<Texture>,
    ) -> anyhow::Result<Self> {
        let shader =
            Shader::new_u32(engine.clone(), include_shader!("rt.hlsl")).context("rt shader")?;

        // set=1 layout:
        //   binding 0: STORAGE_IMAGE    (rt_output, written by raygen)
        //   binding 1: SAMPLED_IMAGE    (gbuf_normal)
        //   binding 2: SAMPLED_IMAGE    (gbuf_albedo)
        //   binding 3: SAMPLED_IMAGE    (gbuf_roughness_metal)
        //   binding 4: SAMPLED_IMAGE    (depth_target, for world-position reconstruction)
        let rt_ds_layout = DescriptorSetLayout::new(
            engine.clone(),
            &[
                DescriptorSetBinding {
                    typ: vk::DescriptorType::STORAGE_IMAGE,
                    binding: 0,
                    stage: vk::ShaderStageFlags::RAYGEN_KHR,
                    count: 1,
                },
                DescriptorSetBinding {
                    typ: vk::DescriptorType::SAMPLED_IMAGE,
                    binding: 1,
                    stage: vk::ShaderStageFlags::RAYGEN_KHR,
                    count: 1,
                },
                DescriptorSetBinding {
                    typ: vk::DescriptorType::SAMPLED_IMAGE,
                    binding: 2,
                    stage: vk::ShaderStageFlags::RAYGEN_KHR,
                    count: 1,
                },
                DescriptorSetBinding {
                    typ: vk::DescriptorType::SAMPLED_IMAGE,
                    binding: 3,
                    stage: vk::ShaderStageFlags::RAYGEN_KHR,
                    count: 1,
                },
                DescriptorSetBinding {
                    typ: vk::DescriptorType::SAMPLED_IMAGE,
                    binding: 4,
                    stage: vk::ShaderStageFlags::RAYGEN_KHR,
                    count: 1,
                },
            ],
        )
        .context("RT DS layout")?;

        let push_range = [vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::RAYGEN_KHR
                | vk::ShaderStageFlags::MISS_KHR
                | vk::ShaderStageFlags::CLOSEST_HIT_KHR,
            offset: 0,
            size: std::mem::size_of::<RtPushConstants>() as u32,
        }];

        let pipeline_layout = PipelineLayout::new(
            engine.clone(),
            vec![
                resource_manager.get_meta_descriptor_set_layout().clone(),
                rt_ds_layout.clone(),
            ],
            &push_range,
        )
        .context("RT pipeline layout")?;

        // Build RT pipeline with 6 shader groups:
        //   group 0: raygen_main           (raygen)
        //   group 1: shadow_miss_main      (miss, SBT miss index 0)
        //   group 2: primary_miss_main     (miss, SBT miss index 1 — unused placeholder)
        //   group 3: bounce_miss_main      (miss, SBT miss index 2 — bounce ray sky fallback)
        //   group 4: primary_hit_main      (closesthit, SBT hit index 0 — shadow dummy, never invoked)
        //   group 5: bounce_hit_main       (closesthit, SBT hit index 1 — bounce surface data)
        //
        // TraceRay for shadow rays:  sbtRecordOffset=0 → group 4 (primary_hit_main, skipped via RAY_FLAG_SKIP_CLOSEST_HIT_SHADER)
        // TraceRay for bounce rays:  sbtRecordOffset=1 → group 5 (bounce_hit_main), missIndex=2 → group 3 (bounce_miss_main)
        let stages = [
            RtShaderStage {
                stage: vk::ShaderStageFlags::RAYGEN_KHR,
                shader: &shader,
                entry_point: c"raygen_main",
            },
            RtShaderStage {
                stage: vk::ShaderStageFlags::MISS_KHR,
                shader: &shader,
                entry_point: c"shadow_miss_main",
            },
            RtShaderStage {
                stage: vk::ShaderStageFlags::MISS_KHR,
                shader: &shader,
                entry_point: c"primary_miss_main",
            },
            RtShaderStage {
                stage: vk::ShaderStageFlags::MISS_KHR,
                shader: &shader,
                entry_point: c"bounce_miss_main",
            },
            RtShaderStage {
                stage: vk::ShaderStageFlags::CLOSEST_HIT_KHR,
                shader: &shader,
                entry_point: c"primary_hit_main",
            },
            RtShaderStage {
                stage: vk::ShaderStageFlags::CLOSEST_HIT_KHR,
                shader: &shader,
                entry_point: c"bounce_hit_main",
            },
        ];

        let groups = [
            // group 0: raygen
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(0)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
            // group 1: shadow miss
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(1)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
            // group 2: primary miss (placeholder)
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(2)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
            // group 3: bounce miss
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(3)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
            // group 4: primary closesthit (shadow dummy — never actually invoked)
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
                .general_shader(vk::SHADER_UNUSED_KHR)
                .closest_hit_shader(4)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
            // group 5: bounce closesthit — fetches vertex/material data, fills BouncePayload
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
                .general_shader(vk::SHADER_UNUSED_KHR)
                .closest_hit_shader(5)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
        ];

        let pipeline = RayTracingPipeline::new_custom(
            engine.clone(),
            pipeline_layout.clone(),
            &stages,
            &groups,
            RtSbtDesc {
                num_miss_groups: 3,
                num_hit_groups: 2,
            },
            4, // raygen → bounce1(+shadow) → bounce2(+shadow): max depth = 4
        )
        .context("RT pipeline")?;

        let rt_output = Texture::new(
            engine.clone(),
            &TextureCreateInfo {
                image_type: vk::ImageType::TYPE_2D,
                update_frequency: UpdateFrequency::PerFrame,
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
        descriptor_set.write_image(&WriteImageInfo {
            binding: 1,
            image: &*gbuf_normal,
            layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
            sampler: None,
            array_index: 0,
        });
        descriptor_set.write_image(&WriteImageInfo {
            binding: 2,
            image: &*gbuf_albedo,
            layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
            sampler: None,
            array_index: 0,
        });
        descriptor_set.write_image(&WriteImageInfo {
            binding: 3,
            image: &*gbuf_roughness_metal,
            layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
            sampler: None,
            array_index: 0,
        });
        descriptor_set.write_image(&WriteImageInfo {
            binding: 4,
            image: &*depth_target,
            layout: vk::ImageLayout::DEPTH_READ_ONLY_OPTIMAL,
            descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
            sampler: None,
            array_index: 0,
        });
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

        cb.cmd_image_barrier(
            engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::Nothing],
                next_accesses: &[AccessType::AnyShaderWrite],
                previous_layout: vk::ImageLayout::UNDEFINED,
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
            frame_index: engine.get_current_frame_index() as u32,
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

        Ok(())
    }
}
