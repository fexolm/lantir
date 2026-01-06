use crate::vulkan::device::VulkanDevice;
use crate::vulkan::frame::VulkanFrame;
use crate::vulkan::instance::VulkanInstance;
use crate::vulkan::surface::VulkanSurface;
use crate::vulkan::swapchain::{VulkanSwapchain, VulkanSwapchainImage};
use crate::RenderingEngineConfig;
use ash::vk;
use std::cell::Cell;
use winit::window::Window;

pub struct VulkanEngine {
    pub instance: VulkanInstance,
    pub surface: VulkanSurface,
    pub device: VulkanDevice,
    pub swapchain: VulkanSwapchain,
    pub frames: Vec<VulkanFrame>,

    current_frame: Cell<usize>,
}

impl VulkanEngine {
    pub fn new(window: &Window, config: &RenderingEngineConfig) -> anyhow::Result<Self> {
        unsafe {
            let instance = VulkanInstance::new(window, config.debug)?;
            let surface = VulkanSurface::new(&instance, window)?;
            let device = VulkanDevice::new(&instance, &surface)?;
            let swapchain = VulkanSwapchain::new(&instance, &device, &surface)?;

            let frames = (0..config.frames_in_flight)
                .map(|_| VulkanFrame::new(&device))
                .collect::<Result<_, _>>()?;

            Ok(Self {
                instance,
                surface,
                device,
                swapchain,
                frames,
                current_frame: Cell::new(0),
            })
        }
    }

    pub fn begin_frame(&self) -> anyhow::Result<&VulkanFrame> {
        self.current_frame
            .set((self.current_frame.get() + 1) % self.frames.len());

        let frame = &self.frames[self.current_frame.get()];

        unsafe {
            frame.render_command_buffer.reset(&self.device)?;
        }

        Ok(frame)
    }

    pub fn submit_and_present(
        &self,
        frame: &VulkanFrame,
        swapchain_image: &VulkanSwapchainImage,
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

    pub fn acquire_swapchain_image(
        &self,
        frame: &VulkanFrame,
    ) -> anyhow::Result<VulkanSwapchainImage> {
        unsafe { self.swapchain.acquire_next_image(&frame) }
    }
}

impl Drop for VulkanEngine {
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
