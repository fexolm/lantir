use crate::device::Device;
use crate::resource::DeferDrop;
use crate::{CommandBuffer, RenderEngine};
use ash::vk;
use std::sync::Mutex;

pub struct RenderFrame {
    pub(crate) render_command_buffer: CommandBuffer,
    pub(crate) swapchain_acquire_semaphore: vk::Semaphore,

    pub(crate) deletion_queue: Mutex<Vec<Box<dyn DeferDrop>>>,
}

impl RenderFrame {
    pub(crate) unsafe fn new(device: &Device) -> anyhow::Result<Self> {
        let render_command_buffer = CommandBuffer::new(device, device.universal_pool)?;

        let swapchain_acquire_semaphore =
            device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;

        Ok(RenderFrame {
            render_command_buffer,
            swapchain_acquire_semaphore,
            deletion_queue: Mutex::new(Vec::new()),
        })
    }

    pub(crate) unsafe fn destroy(&self, engine: &RenderEngine) {
        self.cleanup_resources(engine);

        engine
            .device
            .destroy_semaphore(self.swapchain_acquire_semaphore, None);
        self.render_command_buffer.destroy(&engine.device);
    }

    pub fn get_render_command_buffer(&self) -> &CommandBuffer {
        &self.render_command_buffer
    }

    pub(crate) fn enqueue_drop(&self, resource: impl DeferDrop + 'static) {
        let mut queue = self.deletion_queue.lock().unwrap();
        queue.push(Box::new(resource));
    }

    pub(crate) fn cleanup_resources(&self, engine: &RenderEngine) {
        let mut queue = {
            let mut queue = self.deletion_queue.lock().unwrap();
            queue.drain(..).collect::<Vec<_>>()
        };

        for mut resource in queue.drain(..) {
            resource.destroy(engine);
        }
    }
}
