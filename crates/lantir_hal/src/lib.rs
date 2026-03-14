#![allow(unsafe_op_in_unsafe_fn)]
#![warn(clippy::all)]

pub mod acceleration_structure;
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
pub use barriers::{AccessType, ImageBarrier, GlobalBarrier, BufferBarrier};
pub use buffer::{Buffer, BufferCreateInfo};
pub use command_buffer::{
    CommandBuffer, CopyBufferImageInfo, CopyImageInfo, RenderingAttachmentInfo, RenderingInfo,
};
pub use descriptor_set::{
    DescriptorSet, DescriptorSetBinding, DescriptorSetLayout, WriteBufferInfo, WriteImageInfo,
    WriteSamplerInfo,
};
pub use engine::{RenderEngine, RenderEngineConfig};
pub use frame::RenderFrame;
pub use image::{Image, Sampler, SamplerInfo, Texture, TextureCreateInfo, UpdateFrequency};
pub use pipeline::{
    BlendingMode, ComputePipeline, GraphicsPipeline, GraphicsPipelineCreateInfo, PipelineLayout,
    RayTracingPipeline, Specialization,
};
pub use shader::Shader;
pub use swapchain::SwapchainImage;
pub use vk_mem::AllocationCreateFlags;
