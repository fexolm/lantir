use crate::vulkan::instance::VulkanInstance;
use ash::khr::surface;
use ash::vk;
use std::ops::Deref;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

pub struct VulkanSurface {
    surface: vk::SurfaceKHR,
    loader: surface::Instance,
}

impl VulkanSurface {
    pub unsafe fn new(instance: &VulkanInstance, window: &Window) -> anyhow::Result<VulkanSurface> {
        let surface = ash_window::create_surface(
            &instance.entry,
            instance,
            window.display_handle()?.as_raw(),
            window.window_handle()?.as_raw(),
            None,
        )?;

        let loader = surface::Instance::new(&instance.entry, &instance);

        Ok(VulkanSurface { surface, loader })
    }

    pub fn get_raw(&self) -> vk::SurfaceKHR {
        self.surface
    }

    pub unsafe fn destroy(&mut self) {
        self.loader.destroy_surface(self.surface, None);
    }
}

impl Deref for VulkanSurface {
    type Target = surface::Instance;
    fn deref(&self) -> &Self::Target {
        &self.loader
    }
}
