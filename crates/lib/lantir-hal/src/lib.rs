#![allow(unsafe_op_in_unsafe_fn)]

mod barriers;
mod buffer;
mod command_buffer;
mod descriptor_set;
mod device;
mod engine;
mod frame;
mod image;
mod instance;
mod pipeline;
mod resource;
mod shader;
mod surface;
mod swapchain;

pub use ash::vk;
pub use barriers::{AccessType, ImageBarrier};
pub use buffer::{Buffer, BufferCreateInfo};
pub use command_buffer::{CommandBuffer, CopyImageInfo, RenderingAttachmentInfo, RenderingInfo};
pub use descriptor_set::{
    DescriptorSet, DescriptorSetBinding, DescriptorSetLayout, WriteImageInfo,
};
pub use engine::{RenderEngine, RenderEngineConfig};
pub use frame::RenderFrame;
pub use image::{Image, Texture, TextureCreateInfo, UpdateFrequency};
pub use pipeline::{
    BlendingMode, ComputePipeline, GraphicsPipeline, GraphicsPipelineCreateInfo, PipelineLayout,
};
pub use shader::Shader;
pub use swapchain::SwapchainImage;
pub use vk_mem::AllocationCreateFlags;
