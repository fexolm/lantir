use super::vulkan::VulkanInstance;
use std::sync::Arc;
use winit::window::Window;

#[derive(Clone, Copy)]
pub struct RenderBackendConfig {
    pub swapchain_extent: [u32; 2],
    pub debug: bool,
}

pub struct RenderBackend {
    instance: VulkanInstance,
}

impl RenderBackend {
    pub fn new(window: &Window, config: RenderBackendConfig) -> anyhow::Result<Arc<Self>> {
        unsafe {
            let instance = VulkanInstance::new(window, config.debug)?;

            Ok(Arc::new(Self { instance }))
        }
    }
}

impl Drop for RenderBackend {
    fn drop(&mut self) {
        unsafe {
            self.instance.destroy();
        }
    }
}
