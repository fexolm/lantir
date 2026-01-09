use crate::barriers::ImageBarrier;
use crate::barriers::{get_image_memory_barrier, make_subresource_range};
use crate::device::Device;
use crate::{
    Buffer, ComputePipeline, DescriptorSet, GraphicsPipeline, Image, PipelineLayout, RenderEngine,
};
use ash::vk;
use ash::vk::Extent2D;

pub struct CommandBuffer {
    pub command_buffer: vk::CommandBuffer,
    pub submit_fence: vk::Fence,
}

impl CommandBuffer {
    pub(crate) unsafe fn new(device: &Device, pool: vk::CommandPool) -> anyhow::Result<Self> {
        let submit_fence = {
            let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

            device.create_fence(&fence_info, None)?
        };

        let command_buffer = {
            let info = vk::CommandBufferAllocateInfo::default()
                .command_pool(pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);

            device.allocate_command_buffers(&info)?[0]
        };

        Ok(CommandBuffer {
            command_buffer,
            submit_fence,
        })
    }

    pub fn reset(&self, engine: &RenderEngine) -> anyhow::Result<()> {
        unsafe {
            let device = &engine.device;

            device
                .reset_command_buffer(self.command_buffer, vk::CommandBufferResetFlags::empty())?;
        }

        Ok(())
    }

    pub fn begin(&self, engine: &RenderEngine) -> anyhow::Result<()> {
        unsafe {
            engine.device.begin_command_buffer(
                self.command_buffer,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
        }
        Ok(())
    }

    pub fn end(&self, engine: &RenderEngine) -> anyhow::Result<()> {
        unsafe {
            engine.device.end_command_buffer(self.command_buffer)?;
        }
        Ok(())
    }

    pub(crate) unsafe fn destroy(&self, device: &Device) {
        device.destroy_fence(self.submit_fence, None);
    }

    pub fn cmd_image_barrier(&self, engine: &RenderEngine, barrier: &ImageBarrier) {
        let (src_mask, dst_mask, barrier) = get_image_memory_barrier(&engine, barrier);

        let barriers = [barrier];
        unsafe {
            engine.device.cmd_pipeline_barrier(
                self.command_buffer,
                src_mask,
                dst_mask,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barriers,
            );
        }
    }

    pub fn cmd_clear_color(
        &self,
        engine: &RenderEngine,
        image: &dyn Image,
        layout: vk::ImageLayout,
        color: [f32; 4],
        aspect_mask: vk::ImageAspectFlags,
    ) {
        let mut clear_value = vk::ClearColorValue::default();
        clear_value.float32 = color;

        let subresource_ranges = [make_subresource_range(aspect_mask)];
        unsafe {
            engine.device.cmd_clear_color_image(
                self.command_buffer,
                image.get_image(),
                layout,
                &clear_value,
                &subresource_ranges,
            );
        }
    }

    pub fn cmd_copy_image(&self, engine: &RenderEngine, copy_info: &CopyImageInfo) {
        let src_image = copy_info.src_image;
        let dst_image = copy_info.dst_image;

        let blit_regions = [vk::ImageBlit2::default()
            .src_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: copy_info.src_extent.width as i32,
                    y: copy_info.src_extent.height as i32,
                    z: 1,
                },
            ])
            .dst_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: copy_info.dst_extent.width as i32,
                    y: copy_info.dst_extent.height as i32,
                    z: 1,
                },
            ])
            .src_subresource(vk::ImageSubresourceLayers {
                aspect_mask: copy_info.src_aspect_mask,
                base_array_layer: 0,
                layer_count: 1,
                mip_level: 0,
            })
            .dst_subresource(vk::ImageSubresourceLayers {
                aspect_mask: copy_info.dst_aspect_mask,
                base_array_layer: 0,
                layer_count: 1,
                mip_level: 0,
            })];

        let blit_info = vk::BlitImageInfo2::default()
            .src_image(src_image.get_image())
            .src_image_layout(copy_info.src_layout)
            .dst_image(dst_image.get_image())
            .dst_image_layout(copy_info.dst_layout)
            .filter(vk::Filter::LINEAR)
            .regions(&blit_regions);

        unsafe {
            engine
                .device
                .cmd_blit_image2(self.command_buffer, &blit_info);
        }
    }

    pub fn cmd_bind_compute_pipeline(&self, engine: &RenderEngine, pipeline: &ComputePipeline) {
        unsafe {
            engine.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::COMPUTE,
                pipeline.pipeline,
            );
        }
    }

    pub fn cmd_bind_graphics_pipeline(&self, engine: &RenderEngine, pipeline: &GraphicsPipeline) {
        unsafe {
            engine.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.pipeline,
            );
        }
    }

    pub fn cmd_bind_descriptor_sets(
        &self,
        engine: &RenderEngine,
        pipeline_layout: &PipelineLayout,
        descriptor_sets: &[&DescriptorSet],
        bind_point: vk::PipelineBindPoint,
        first_set: u32,
    ) {
        unsafe {
            let sets = descriptor_sets.iter().map(|s| s.get()).collect::<Vec<_>>();

            engine.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                bind_point,
                pipeline_layout.layout,
                first_set,
                &sets,
                &[],
            )
        };
    }

    pub fn cmd_dispatch(
        &self,
        engine: &RenderEngine,
        group_count_x: u32,
        group_count_y: u32,
        group_count_z: u32,
    ) {
        unsafe {
            engine.device.cmd_dispatch(
                self.command_buffer,
                group_count_x,
                group_count_y,
                group_count_z,
            );
        }
    }

    pub fn cmd_push_constants<T: Sized>(
        &self,
        engine: &RenderEngine,
        layout: &PipelineLayout,
        stage: vk::ShaderStageFlags,
        offset: u32,
        data: &T,
    ) {
        unsafe {
            let bytes = std::slice::from_raw_parts((data as *const T) as *const u8, size_of::<T>());

            engine.device.cmd_push_constants(
                self.command_buffer,
                layout.layout,
                stage,
                offset,
                bytes,
            );
        }
    }

    pub fn cmd_begin_rendering(&self, engine: &RenderEngine, render_info: &RenderingInfo) {
        let color_attachments: Vec<vk::RenderingAttachmentInfo> = render_info
            .color_attachments
            .iter()
            .map(|att| att.to_vk())
            .collect();

        let mut vk_info = vk::RenderingInfo::default()
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: render_info.extent,
            })
            .layer_count(1)
            .color_attachments(&color_attachments);

        let vk_depth_attachment;
        if let Some(depth_attachment) = render_info.depth_attachment {
            vk_depth_attachment = depth_attachment.to_vk();
            vk_info = vk_info.depth_attachment(&vk_depth_attachment);
        }

        unsafe {
            engine
                .device
                .cmd_begin_rendering(self.command_buffer, &vk_info)
        }
    }

    pub fn cmd_set_viewport(&self, engine: &RenderEngine, extent: Extent2D) {
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };

        unsafe {
            engine
                .device
                .cmd_set_viewport(self.command_buffer, 0, &[viewport]);
        }
    }

    pub fn cmd_set_scissor(&self, engine: &RenderEngine, extent: Extent2D) {
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };

        unsafe {
            engine
                .device
                .cmd_set_scissor(self.command_buffer, 0, &[scissor]);
        }
    }

    pub fn cmd_draw(&self, engine: &RenderEngine, vertex_count: u32, instance_count: u32) {
        unsafe {
            engine
                .device
                .cmd_draw(self.command_buffer, vertex_count, instance_count, 0, 0);
        }
    }

    pub fn cmd_draw_indexed(&self, engine: &RenderEngine, index_count: u32, instance_count: u32) {
        unsafe {
            engine.device.cmd_draw_indexed(
                self.command_buffer,
                index_count,
                instance_count,
                0,
                0,
                0,
            );
        }
    }

    pub fn cmd_end_rendering(&self, engine: &RenderEngine) {
        unsafe {
            engine.device.cmd_end_rendering(self.command_buffer);
        }
    }

    pub fn cmd_copy_buffer(
        &self,
        engine: &RenderEngine,
        src_buffer: &Buffer,
        dst_buffer: &Buffer,
        copy_region: vk::BufferCopy,
    ) {
        unsafe {
            engine.device.cmd_copy_buffer(
                self.command_buffer,
                src_buffer.get_buffer(),
                dst_buffer.get_buffer(),
                &[copy_region],
            );
        }
    }

    pub fn cmd_copy_buffer_to_image(&self, engine: &RenderEngine, copy_info: &CopyBufferImageInfo) {
        let copy_region = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(copy_info.image_aspect_mask)
                    .mip_level(0)
                    .base_array_layer(0)
                    .layer_count(1),
            )
            .image_extent(copy_info.image_extent);

        unsafe {
            engine.device.cmd_copy_buffer_to_image(
                self.command_buffer,
                copy_info.buffer.get_buffer(),
                copy_info.image.get_image(),
                copy_info.image_layout,
                &[copy_region],
            );
        }
    }

    pub fn cmd_bind_index_buffer(
        &self,
        engine: &RenderEngine,
        index_buffer: &Buffer,
        index_type: vk::IndexType,
    ) {
        unsafe {
            engine.device.cmd_bind_index_buffer(
                self.command_buffer,
                index_buffer.get_buffer(),
                0,
                index_type,
            );
        }
    }
}

pub struct CopyImageInfo<'i> {
    pub src_image: &'i dyn Image,
    pub src_layout: vk::ImageLayout,
    pub src_aspect_mask: vk::ImageAspectFlags,
    pub src_extent: Extent2D,

    pub dst_image: &'i dyn Image,
    pub dst_layout: vk::ImageLayout,
    pub dst_aspect_mask: vk::ImageAspectFlags,
    pub dst_extent: Extent2D,
}

pub struct CopyBufferImageInfo<'i> {
    pub buffer: &'i Buffer,

    pub image: &'i dyn Image,
    pub image_layout: vk::ImageLayout,
    pub image_aspect_mask: vk::ImageAspectFlags,
    pub image_extent: vk::Extent3D,
}

pub struct RenderingAttachmentInfo<'i> {
    pub image: &'i dyn Image,
    pub layout: vk::ImageLayout,
}

impl RenderingAttachmentInfo<'_> {
    pub fn to_vk(&self) -> vk::RenderingAttachmentInfo {
        vk::RenderingAttachmentInfo::default()
            .image_view(self.image.get_image_view())
            .image_layout(self.layout)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
    }
}

pub struct RenderingInfo<'i> {
    pub color_attachments: &'i [RenderingAttachmentInfo<'i>],
    pub depth_attachment: Option<&'i RenderingAttachmentInfo<'i>>,
    pub extent: Extent2D,
}
