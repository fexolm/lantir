use crate::device::Device;
use crate::frame::RenderFrame;
use crate::instance::Instance;
use crate::surface::Surface;
use crate::swapchain::{Swapchain, SwapchainImage};
use ash::vk;
use std::cell::Cell;
use std::sync::Arc;
use winit::window::Window;

pub struct RenderEngine {
    pub instance: Instance,
    pub surface: Surface,
    pub device: Device,
    pub swapchain: Swapchain,
    pub frames: Vec<RenderFrame>,

    current_frame: Cell<usize>,
}

pub struct RenderEngineConfig {
    pub debug: bool,
    pub frames_in_flight: usize,
}

impl RenderEngine {
    pub fn new(window: &Window, config: &RenderEngineConfig) -> anyhow::Result<Arc<Self>> {
        unsafe {
            let instance = Instance::new(window, config.debug)?;
            let surface = Surface::new(&instance, window)?;
            let device = Device::new(&instance, &surface)?;
            let swapchain = Swapchain::new(&instance, &device, &surface)?;

            let frames = (0..config.frames_in_flight)
                .map(|_| RenderFrame::new(&device))
                .collect::<Result<_, _>>()?;

            Ok(Arc::new(Self {
                instance,
                surface,
                device,
                swapchain,
                frames,
                current_frame: Cell::new(0),
            }))
        }
    }

    pub fn begin_frame(&self) -> anyhow::Result<&RenderFrame> {
        self.current_frame
            .set((self.current_frame.get() + 1) % self.frames.len());

        let frame = &self.frames[self.current_frame.get()];

        frame.render_command_buffer.reset(self)?;

        Ok(frame)
    }

    pub fn submit_and_present(
        &self,
        frame: &RenderFrame,
        swapchain_image: &SwapchainImage,
    ) -> anyhow::Result<()> {
        unsafe {
            self.device.submit(
                &frame.render_command_buffer,
                frame.swapchain_acquire_semaphore,
                swapchain_image.render_finished_semaphore,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags2::TRANSFER,
                vk::PipelineStageFlags2::ALL_GRAPHICS,
                self.device.universal_queue,
            )?;

            self.swapchain.present(&self.device, swapchain_image)?;
        }

        Ok(())
    }

    pub fn acquire_swapchain_image(&self, frame: &RenderFrame) -> anyhow::Result<SwapchainImage> {
        unsafe { self.swapchain.acquire_next_image(&frame) }
    }
}

impl Drop for RenderEngine {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();

            for frame in &mut self.frames {
                frame.destroy(&self.device);
            }

            self.swapchain.destroy(&self.device);
            self.device.destroy();
            self.surface.destroy();
            self.instance.destroy();
        }
    }
}
