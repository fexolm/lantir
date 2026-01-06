use crate::barriers::ImageBarrier;
use crate::barriers::{get_image_memory_barrier, make_subresource_range};
use crate::device::Device;
use crate::{Image, RenderEngine};
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
            device.wait_for_fences(&[self.submit_fence], true, u64::MAX)?;
            device.reset_fences(&[self.submit_fence])?;
            device
                .reset_command_buffer(self.command_buffer, vk::CommandBufferResetFlags::empty())?;
            device.begin_command_buffer(
                self.command_buffer,
                &vk::CommandBufferBeginInfo::default(),
            )?;
        }

        Ok(())
    }

    pub(crate) unsafe fn destroy(&mut self, device: &Device) {
        device.destroy_fence(self.submit_fence, None);
    }

    pub fn cmd_image_barrier(&self, engine: &RenderEngine, barrier: &ImageBarrier) {
        let (src_mask, dst_mask, barrier) = get_image_memory_barrier(&engine.device, barrier);

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
}

pub struct CopyImageInfo {
    pub src_image: &'static dyn Image,
    pub src_layout: vk::ImageLayout,
    pub src_aspect_mask: vk::ImageAspectFlags,
    pub src_extent: Extent2D,

    pub dst_image: &'static dyn Image,
    pub dst_layout: vk::ImageLayout,
    pub dst_aspect_mask: vk::ImageAspectFlags,
    pub dst_extent: Extent2D,
}
