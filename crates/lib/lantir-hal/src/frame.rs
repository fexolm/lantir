use crate::device::Device;
use crate::CommandBuffer;
use ash::vk;

pub struct RenderFrame {
    pub(crate) render_command_buffer: CommandBuffer,
    pub(crate) swapchain_acquire_semaphore: vk::Semaphore,
}

impl RenderFrame {
    pub(crate) unsafe fn new(device: &Device) -> anyhow::Result<Self> {
        let render_command_buffer = CommandBuffer::new(device, device.universal_pool)?;

        let swapchain_acquire_semaphore =
            device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;

        Ok(RenderFrame {
            render_command_buffer,
            swapchain_acquire_semaphore,
        })
    }

    pub(crate) unsafe fn destroy(&mut self, device: &Device) {
        device.destroy_semaphore(self.swapchain_acquire_semaphore, None);
        self.render_command_buffer.destroy(device);
    }
    
    pub fn get_render_command_buffer(&self) -> &CommandBuffer {
        &self.render_command_buffer
    }
}
