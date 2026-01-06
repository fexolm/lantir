#![allow(unsafe_op_in_unsafe_fn)]

pub mod barriers;
mod command_buffer;
mod vulkan;

use crate::vulkan::{VulkanBuffer, VulkanEngine, VulkanFrame, VulkanImage, VulkanSwapchainImage};
use std::sync::Arc;
use winit::window::Window;

pub use crate::command_buffer::CommandBuffer;

pub struct RenderingEngineConfig {
    pub debug: bool,
    pub frames_in_flight: usize,
}

pub struct RenderingEngine {
    imp: VulkanEngine,
}

pub struct Frame<'i> {
    imp: &'i VulkanFrame,
}

pub trait Buffer<'i> {
    fn get_imp(&self) -> &'i dyn VulkanBuffer;
}

pub trait Image<'i> {
    fn get_imp(&'i self) -> &'i dyn VulkanImage;
}

pub struct SwapchainImage {
    imp: VulkanSwapchainImage,
}

impl<'i> Image<'i> for SwapchainImage {
    fn get_imp(&'i self) -> &'i dyn VulkanImage {
        &self.imp as &dyn VulkanImage
    }
}

impl Frame<'_> {
    pub fn get_render_command_buffer(&self) -> CommandBuffer<'_> {
        CommandBuffer {
            imp: &self.imp.render_command_buffer,
        }
    }
}

impl RenderingEngine {
    pub fn new(window: &Window, config: &RenderingEngineConfig) -> anyhow::Result<Arc<Self>> {
        Ok(Arc::new(RenderingEngine {
            imp: VulkanEngine::new(window, &config)?,
        }))
    }

    pub fn begin_frame(&self) -> anyhow::Result<Frame<'_>> {
        Ok(Frame {
            imp: self.imp.begin_frame()?,
        })
    }

    pub fn acquire_swapchain_image(&self, frame: &Frame) -> anyhow::Result<SwapchainImage> {
        Ok(SwapchainImage {
            imp: self.imp.acquire_swapchain_image(&frame.imp)?,
        })
    }

    pub fn submit_and_present(
        &self,
        frame: Frame<'_>,
        swapchain_image: SwapchainImage,
    ) -> anyhow::Result<()> {
        self.imp.submit_and_present(frame.imp, &swapchain_image.imp)
    }
}
