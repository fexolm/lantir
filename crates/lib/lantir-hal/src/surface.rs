use crate::instance::Instance;
use ash::khr::surface;
use ash::vk;
use std::ops::Deref;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

pub struct Surface {
    surface: vk::SurfaceKHR,
    loader: surface::Instance,
}

impl Surface {
    pub unsafe fn new(instance: &Instance, window: &Window) -> anyhow::Result<Surface> {
        let surface = ash_window::create_surface(
            &instance.entry,
            instance,
            window.display_handle()?.as_raw(),
            window.window_handle()?.as_raw(),
            None,
        )?;

        let loader = surface::Instance::new(&instance.entry, &instance);

        Ok(Surface { surface, loader })
    }

    pub fn get_raw(&self) -> vk::SurfaceKHR {
        self.surface
    }

    pub unsafe fn destroy(&mut self) {
        self.loader.destroy_surface(self.surface, None);
    }
}

impl Deref for Surface {
    type Target = surface::Instance;
    fn deref(&self) -> &Self::Target {
        &self.loader
    }
}
