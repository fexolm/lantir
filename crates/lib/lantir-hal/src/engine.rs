use crate::device::Device;
use crate::frame::RenderFrame;
use crate::image::{Texture, TextureCreateInfo};
use crate::instance::Instance;
use crate::resource::ResourceDrop;
use crate::surface::Surface;
use crate::swapchain::{Swapchain, SwapchainImage};
use anyhow::anyhow;
use ash::vk;
use std::sync::{Arc, Mutex};
use vk_mem::{Allocator, AllocatorCreateInfo};
use winit::window::Window;

pub struct RenderEngine {
    pub(crate) swapchain: Swapchain,
    pub(crate) frames: Vec<RenderFrame>,
    current_frame: Mutex<usize>,

    pub(crate) allocator: Allocator,
    pub(crate) device: Device,
    pub(crate) surface: Surface,
    pub(crate) instance: Instance,
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

            let allocator = Allocator::new(AllocatorCreateInfo::new(
                &instance,
                &device,
                device.physical_device,
            ))?;

            let frames = (0..config.frames_in_flight)
                .map(|_| RenderFrame::new(&device))
                .collect::<Result<_, _>>()?;

            Ok(Arc::new(Self {
                instance,
                surface,
                device,
                swapchain,
                frames,
                allocator,
                current_frame: Mutex::new(0),
            }))
        }
    }
    pub fn get_current_frame_index(&self) -> usize {
        let current_frame = self.current_frame.lock().unwrap();

        *current_frame
    }

    pub fn begin_frame(&self) -> anyhow::Result<&RenderFrame> {
        let mut current_frame = self
            .current_frame
            .lock()
            .map_err(|e| anyhow!(e.to_string()))?;

        *current_frame = (*current_frame + 1) % self.frames.len();

        let frame = &self.frames[*current_frame];

        frame.cleanup_resources(&self);
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

    pub fn schedule_resource_release(&self, resource: impl ResourceDrop + 'static) {
        let frame_index = self.get_current_frame_index();
        self.frames[frame_index].enqueue_drop(resource);
    }

    pub fn create_texture(
        self: &Arc<Self>,
        create_info: &TextureCreateInfo,
    ) -> anyhow::Result<Texture> {
        Ok(Texture::new(self.clone(), create_info)?)
    }
}

impl Drop for RenderEngine {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();

            for frame in &self.frames {
                frame.destroy(&self);
            }

            self.swapchain.destroy(&self.device);
        }
    }
}
