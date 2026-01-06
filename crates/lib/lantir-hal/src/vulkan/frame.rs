use crate::vulkan::command_buffer::VulkanCommandBuffer;
use crate::vulkan::device::VulkanDevice;
use ash::vk;

pub struct VulkanFrame {
    pub render_command_buffer: VulkanCommandBuffer,
    pub swapchain_acquire_semaphore: vk::Semaphore,
}

impl VulkanFrame {
    pub unsafe fn new(device: &VulkanDevice) -> anyhow::Result<Self> {
        let render_command_buffer = VulkanCommandBuffer::new(device, device.universal_pool)?;

        let swapchain_acquire_semaphore =
            device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;

        Ok(VulkanFrame {
            render_command_buffer,
            swapchain_acquire_semaphore,
        })
    }

    pub unsafe fn destroy(&mut self, device: &VulkanDevice) {
        device.destroy_semaphore(self.swapchain_acquire_semaphore, None);
        self.render_command_buffer.destroy(device);
    }
}
