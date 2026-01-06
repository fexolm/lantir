use crate::barriers::{AspectMask, ImageBarrier, ImageLayout};
use crate::vulkan::barriers::{convert_layout, get_image_memory_barrier, make_subresource_range};
use crate::vulkan::device::VulkanDevice;
use crate::vulkan::{VulkanEngine, VulkanImage};
use ash::vk;

pub struct VulkanCommandBuffer {
    pub command_buffer: vk::CommandBuffer,
    pub submit_fence: vk::Fence,
}

impl VulkanCommandBuffer {
    pub unsafe fn new(device: &VulkanDevice, pool: vk::CommandPool) -> anyhow::Result<Self> {
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

        Ok(VulkanCommandBuffer {
            command_buffer,
            submit_fence,
        })
    }

    pub unsafe fn reset(&self, device: &VulkanDevice) -> anyhow::Result<()> {
        device.wait_for_fences(&[self.submit_fence], true, u64::MAX)?;
        device.reset_fences(&[self.submit_fence])?;
        device.reset_command_buffer(self.command_buffer, vk::CommandBufferResetFlags::empty())?;
        device.begin_command_buffer(self.command_buffer, &vk::CommandBufferBeginInfo::default())?;

        Ok(())
    }

    pub unsafe fn destroy(&mut self, device: &VulkanDevice) {
        device.destroy_fence(self.submit_fence, None);
    }

    pub fn cmd_image_barrier(&self, engine: &VulkanEngine, barrier: &ImageBarrier) {
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
        engine: &VulkanEngine,
        image: &dyn VulkanImage,
        layout: ImageLayout,
        color: [f32; 4],
        aspect_mask: AspectMask,
    ) {
        let mut clear_value = vk::ClearColorValue::default();
        clear_value.float32 = color;
        
        let subresource_ranges = [make_subresource_range(aspect_mask)];
        unsafe {
            engine.device.cmd_clear_color_image(
                self.command_buffer,
                image.get_image(),
                convert_layout(layout),
                &clear_value,
                &subresource_ranges,
            );
        }
    }
}
