use ash::vk;

// MIT License
//
// Copyright (c) 2018 Graham Wihlidal
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use crate::RenderEngine;
use crate::buffer::Buffer;
use crate::image::Image;

/// Defines all potential resource usages
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub enum AccessType {
    /// No access. Useful primarily for initialization
    #[default]
    Nothing,

    /// Command buffer read operation as defined by `NVX_device_generated_commands`
    CommandBufferReadNVX,

    /// Read as an indirect buffer for drawing or dispatch
    IndirectBuffer,

    /// Read as an index buffer for drawing
    IndexBuffer,

    /// Read as a vertex buffer for drawing
    VertexBuffer,

    /// Read as a uniform buffer in a vertex shader
    VertexShaderReadUniformBuffer,

    /// Read as a sampled image/uniform texel buffer in a vertex shader
    VertexShaderReadSampledImageOrUniformTexelBuffer,

    /// Read as any other resource in a vertex shader
    VertexShaderReadOther,

    /// Read as a uniform buffer in a tessellation control shader
    TessellationControlShaderReadUniformBuffer,

    /// Read as a sampled image/uniform texel buffer in a tessellation control shader
    TessellationControlShaderReadSampledImageOrUniformTexelBuffer,

    /// Read as any other resource in a tessellation control shader
    TessellationControlShaderReadOther,

    /// Read as a uniform buffer in a tessellation evaluation shader
    TessellationEvaluationShaderReadUniformBuffer,

    /// Read as a sampled image/uniform texel buffer in a tessellation evaluation shader
    TessellationEvaluationShaderReadSampledImageOrUniformTexelBuffer,

    /// Read as any other resource in a tessellation evaluation shader
    TessellationEvaluationShaderReadOther,

    /// Read as a uniform buffer in a geometry shader
    GeometryShaderReadUniformBuffer,

    /// Read as a sampled image/uniform texel buffer in a geometry shader
    GeometryShaderReadSampledImageOrUniformTexelBuffer,

    /// Read as any other resource in a geometry shader
    GeometryShaderReadOther,

    /// Read as a uniform buffer in a fragment shader
    FragmentShaderReadUniformBuffer,

    /// Read as a sampled image/uniform texel buffer in a fragment shader
    FragmentShaderReadSampledImageOrUniformTexelBuffer,

    /// Read as an input attachment with a color format in a fragment shader
    FragmentShaderReadColorInputAttachment,

    /// Read as an input attachment with a depth/stencil format in a fragment shader
    FragmentShaderReadDepthStencilInputAttachment,

    /// Read as any other resource in a fragment shader
    FragmentShaderReadOther,

    /// Read by blending/logic operations or subpass load operations
    ColorAttachmentRead,

    /// Read by depth/stencil tests or subpass load operations
    DepthStencilAttachmentRead,

    /// Read as a uniform buffer in a compute shader
    ComputeShaderReadUniformBuffer,

    /// Read as a sampled image/uniform texel buffer in a compute shader
    ComputeShaderReadSampledImageOrUniformTexelBuffer,

    /// Read as any other resource in a compute shader
    ComputeShaderReadOther,

    /// Read as a uniform buffer in any shader
    AnyShaderReadUniformBuffer,

    /// Read as a uniform buffer in any shader, or a vertex buffer
    AnyShaderReadUniformBufferOrVertexBuffer,

    /// Read as a sampled image in any shader
    AnyShaderReadSampledImageOrUniformTexelBuffer,

    /// Read as any other resource (excluding attachments) in any shader
    AnyShaderReadOther,

    /// Read as the source of a transfer operation
    TransferRead,

    /// Read on the host
    HostRead,

    /// Read by the presentation engine (i.e. `vkQueuePresentKHR`)
    Present,

    /// Command buffer write operation as defined by `NVX_device_generated_commands`
    CommandBufferWriteNVX,

    /// Written as any resource in a vertex shader
    VertexShaderWrite,

    /// Written as any resource in a tessellation control shader
    TessellationControlShaderWrite,

    /// Written as any resource in a tessellation evaluation shader
    TessellationEvaluationShaderWrite,

    /// Written as any resource in a geometry shader
    GeometryShaderWrite,

    /// Written as any resource in a fragment shader
    FragmentShaderWrite,

    /// Written as a color attachment during rendering, or via a subpass store op
    ColorAttachmentWrite,

    /// Written as a depth/stencil attachment during rendering, or via a subpass store op
    DepthStencilAttachmentWrite,

    /// Written as a depth aspect of a depth/stencil attachment during rendering, whilst the
    /// stencil aspect is read-only. Requires `VK_KHR_maintenance2` to be enabled.
    DepthAttachmentWriteStencilReadOnly,

    /// Written as a stencil aspect of a depth/stencil attachment during rendering, whilst the
    /// depth aspect is read-only. Requires `VK_KHR_maintenance2` to be enabled.
    StencilAttachmentWriteDepthReadOnly,

    /// Written as any resource in a compute shader
    ComputeShaderWrite,

    /// Written as any resource in any shader
    AnyShaderWrite,

    /// Written as the destination of a transfer operation
    TransferWrite,

    /// Written on the host
    HostWrite,

    /// Read or written as a color attachment during rendering
    ColorAttachmentReadWrite,

    /// Covers any access - useful for debug, generally avoid for performance reasons
    General,

    /// Read as a sampled image/uniform texel buffer in a ray tracing shader
    RayTracingShaderReadSampledImageOrUniformTexelBuffer,

    /// Read as an input attachment with a color format in a ray tracing shader
    RayTracingShaderReadColorInputAttachment,

    /// Read as an input attachment with a depth/stencil format in a ray tracing shader
    RayTracingShaderReadDepthStencilInputAttachment,

    /// Read as an acceleration structure in a ray tracing shader
    RayTracingShaderReadAccelerationStructure,

    /// Read as any other resource in a ray tracing shader
    RayTracingShaderReadOther,

    /// Written as an acceleration structure during acceleration structure building
    AccelerationStructureBuildWrite,

    /// Read as an acceleration structure during acceleration structure building (e.g. a BLAS when building a TLAS)
    AccelerationStructureBuildRead,

    // Written as a buffer during acceleration structure building (e.g. a staging buffer)
    AccelerationStructureBufferWrite,

    /// Read as an acceleration structure in a compute shader (ray queries)
    ComputeShaderReadAccelerationStructure,
}

/// Global barriers define a set of accesses on multiple resources at once.
/// If a buffer or image doesn't require a queue ownership transfer, or an image
/// doesn't require a layout transition (e.g. you're using one of the
/// `ImageLayout::General*` layouts) then a global barrier should be preferred.
///
/// Simply define the previous and next access types of resources affected.
pub struct GlobalBarrier<'a> {
    pub previous_accesses: &'a [AccessType],
    pub next_accesses: &'a [AccessType],
}

/// Buffer barriers should only be used when a queue family ownership transfer
/// is required - prefer global barriers at all other times.
///
/// Access types are defined in the same way as for a global memory barrier, but
/// they only affect the buffer range identified by `buffer`, `offset` and `size`,
/// rather than all resources.
///
/// `src_queue_family_index` and `dst_queue_family_index` will be passed unmodified
/// into a buffer memory barrier.
///
/// A buffer barrier defining a queue ownership transfer needs to be executed
/// twice - once by a queue in the source queue family, and then once again by a
/// queue in the destination queue family, with a semaphore guaranteeing
/// execution order between them.
pub struct BufferBarrier<'a> {
    pub previous_accesses: &'a [AccessType],
    pub next_accesses: &'a [AccessType],
    pub buffer: &'a Buffer,
    pub offset: usize,
    pub size: usize,
}

pub struct ImageBarrier<'a> {
    pub previous_accesses: &'a [AccessType],
    pub next_accesses: &'a [AccessType],
    pub previous_layout: vk::ImageLayout,
    pub next_layout: vk::ImageLayout,
    pub image: &'a dyn Image,
    pub aspect_mask: vk::ImageAspectFlags,
}

pub fn get_image_memory_barrier<'a>(
    engine: &RenderEngine,
    barrier: &ImageBarrier<'a>,
) -> (
    vk::PipelineStageFlags,
    vk::PipelineStageFlags,
    vk::ImageMemoryBarrier<'a>,
) {
    let device = &engine.device;
    let mut src_stages = vk::PipelineStageFlags::empty();
    let mut dst_stages = vk::PipelineStageFlags::empty();

    let mut image_barrier = vk::ImageMemoryBarrier {
        src_queue_family_index: device.universal_family,
        dst_queue_family_index: device.universal_family,
        image: barrier.image.get_image(),
        subresource_range: make_subresource_range(barrier.aspect_mask),
        ..Default::default()
    };

    for previous_access in barrier.previous_accesses {
        let previous_info = get_access_info(*previous_access);

        src_stages |= previous_info.stage_mask;

        // Add appropriate availability operations - for writes only.
        if is_write_access(*previous_access) {
            image_barrier.src_access_mask |= previous_info.access_mask;
        }

        image_barrier.old_layout = barrier.previous_layout;
    }

    for next_access in barrier.next_accesses {
        let next_info = get_access_info(*next_access);

        dst_stages |= next_info.stage_mask;

        // Add visibility operations as necessary.
        // If the src access mask, this is a WAR hazard (or for some reason a "RAR"),
        // so the dst access mask can be safely zeroed as these don't need visibility.
        if image_barrier.src_access_mask != vk::AccessFlags::empty() {
            image_barrier.dst_access_mask |= next_info.access_mask;
        }

        image_barrier.new_layout = barrier.next_layout;
    }

    // Ensure that the stage masks are valid if no stages were determined
    if src_stages == vk::PipelineStageFlags::empty() {
        src_stages = vk::PipelineStageFlags::TOP_OF_PIPE;
    }

    if dst_stages == vk::PipelineStageFlags::empty() {
        dst_stages = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
    }

    (src_stages, dst_stages, image_barrier)
}

pub fn make_subresource_range(aspect_mask: vk::ImageAspectFlags) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(aspect_mask)
        .base_mip_level(0)
        .level_count(vk::REMAINING_MIP_LEVELS)
        .base_array_layer(0)
        .layer_count(vk::REMAINING_ARRAY_LAYERS)
}

struct AccessInfo {
    pub(crate) stage_mask: vk::PipelineStageFlags,
    pub(crate) access_mask: vk::AccessFlags,
}

fn get_access_info(access_type: AccessType) -> AccessInfo {
    match access_type {
        AccessType::Nothing => AccessInfo {
            stage_mask: vk::PipelineStageFlags::empty(),
            access_mask: vk::AccessFlags::empty(),
        },
        AccessType::CommandBufferReadNVX => AccessInfo {
            stage_mask: vk::PipelineStageFlags::COMMAND_PREPROCESS_NV,
            access_mask: vk::AccessFlags::COMMAND_PREPROCESS_READ_NV,
        },
        AccessType::IndirectBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::DRAW_INDIRECT,
            access_mask: vk::AccessFlags::INDIRECT_COMMAND_READ,
        },
        AccessType::IndexBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::VERTEX_INPUT,
            access_mask: vk::AccessFlags::INDEX_READ,
        },
        AccessType::VertexBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::VERTEX_INPUT,
            access_mask: vk::AccessFlags::VERTEX_ATTRIBUTE_READ,
        },
        AccessType::VertexShaderReadUniformBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::VERTEX_SHADER,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::VertexShaderReadSampledImageOrUniformTexelBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::VERTEX_SHADER,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::VertexShaderReadOther => AccessInfo {
            stage_mask: vk::PipelineStageFlags::VERTEX_SHADER,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::TessellationControlShaderReadUniformBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::TESSELLATION_CONTROL_SHADER,
            access_mask: vk::AccessFlags::UNIFORM_READ,
        },
        AccessType::TessellationControlShaderReadSampledImageOrUniformTexelBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::TESSELLATION_CONTROL_SHADER,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::TessellationControlShaderReadOther => AccessInfo {
            stage_mask: vk::PipelineStageFlags::TESSELLATION_CONTROL_SHADER,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::TessellationEvaluationShaderReadUniformBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::TESSELLATION_EVALUATION_SHADER,
            access_mask: vk::AccessFlags::UNIFORM_READ,
        },
        AccessType::TessellationEvaluationShaderReadSampledImageOrUniformTexelBuffer => {
            AccessInfo {
                stage_mask: vk::PipelineStageFlags::TESSELLATION_EVALUATION_SHADER,
                access_mask: vk::AccessFlags::SHADER_READ,
            }
        }
        AccessType::TessellationEvaluationShaderReadOther => AccessInfo {
            stage_mask: vk::PipelineStageFlags::TESSELLATION_EVALUATION_SHADER,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::GeometryShaderReadUniformBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::GEOMETRY_SHADER,
            access_mask: vk::AccessFlags::UNIFORM_READ,
        },
        AccessType::GeometryShaderReadSampledImageOrUniformTexelBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::GEOMETRY_SHADER,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::GeometryShaderReadOther => AccessInfo {
            stage_mask: vk::PipelineStageFlags::GEOMETRY_SHADER,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::FragmentShaderReadUniformBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::FRAGMENT_SHADER,
            access_mask: vk::AccessFlags::UNIFORM_READ,
        },
        AccessType::FragmentShaderReadSampledImageOrUniformTexelBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::FRAGMENT_SHADER,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::FragmentShaderReadColorInputAttachment => AccessInfo {
            stage_mask: vk::PipelineStageFlags::FRAGMENT_SHADER,
            access_mask: vk::AccessFlags::INPUT_ATTACHMENT_READ,
        },
        AccessType::FragmentShaderReadDepthStencilInputAttachment => AccessInfo {
            stage_mask: vk::PipelineStageFlags::FRAGMENT_SHADER,
            access_mask: vk::AccessFlags::INPUT_ATTACHMENT_READ,
        },
        AccessType::FragmentShaderReadOther => AccessInfo {
            stage_mask: vk::PipelineStageFlags::FRAGMENT_SHADER,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::ColorAttachmentRead => AccessInfo {
            stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            access_mask: vk::AccessFlags::COLOR_ATTACHMENT_READ,
        },
        AccessType::DepthStencilAttachmentRead => AccessInfo {
            stage_mask: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            access_mask: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
        },
        AccessType::ComputeShaderReadUniformBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::COMPUTE_SHADER,
            access_mask: vk::AccessFlags::UNIFORM_READ,
        },
        AccessType::ComputeShaderReadSampledImageOrUniformTexelBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::COMPUTE_SHADER,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::ComputeShaderReadOther => AccessInfo {
            stage_mask: vk::PipelineStageFlags::COMPUTE_SHADER,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::AnyShaderReadUniformBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::ALL_COMMANDS,
            access_mask: vk::AccessFlags::UNIFORM_READ,
        },
        AccessType::AnyShaderReadUniformBufferOrVertexBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::ALL_COMMANDS,
            access_mask: vk::AccessFlags::UNIFORM_READ | vk::AccessFlags::VERTEX_ATTRIBUTE_READ,
        },
        AccessType::AnyShaderReadSampledImageOrUniformTexelBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::ALL_COMMANDS,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::AnyShaderReadOther => AccessInfo {
            stage_mask: vk::PipelineStageFlags::ALL_COMMANDS,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::TransferRead => AccessInfo {
            stage_mask: vk::PipelineStageFlags::TRANSFER,
            access_mask: vk::AccessFlags::TRANSFER_READ,
        },
        AccessType::HostRead => AccessInfo {
            stage_mask: vk::PipelineStageFlags::HOST,
            access_mask: vk::AccessFlags::HOST_READ,
        },
        AccessType::Present => AccessInfo {
            stage_mask: vk::PipelineStageFlags::empty(),
            access_mask: vk::AccessFlags::empty(),
        },
        AccessType::CommandBufferWriteNVX => AccessInfo {
            stage_mask: vk::PipelineStageFlags::COMMAND_PREPROCESS_NV,
            access_mask: vk::AccessFlags::COMMAND_PREPROCESS_WRITE_NV,
        },
        AccessType::VertexShaderWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::VERTEX_SHADER,
            access_mask: vk::AccessFlags::SHADER_WRITE,
        },
        AccessType::TessellationControlShaderWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::TESSELLATION_CONTROL_SHADER,
            access_mask: vk::AccessFlags::SHADER_WRITE,
        },
        AccessType::TessellationEvaluationShaderWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::TESSELLATION_EVALUATION_SHADER,
            access_mask: vk::AccessFlags::SHADER_WRITE,
        },
        AccessType::GeometryShaderWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::GEOMETRY_SHADER,
            access_mask: vk::AccessFlags::SHADER_WRITE,
        },
        AccessType::FragmentShaderWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::FRAGMENT_SHADER,
            access_mask: vk::AccessFlags::SHADER_WRITE,
        },
        AccessType::ColorAttachmentWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            access_mask: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        },
        AccessType::DepthStencilAttachmentWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            access_mask: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        },
        AccessType::DepthAttachmentWriteStencilReadOnly => AccessInfo {
            stage_mask: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            access_mask: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
        },
        AccessType::StencilAttachmentWriteDepthReadOnly => AccessInfo {
            stage_mask: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            access_mask: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
        },
        AccessType::ComputeShaderWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::COMPUTE_SHADER,
            access_mask: vk::AccessFlags::SHADER_WRITE,
        },
        AccessType::AnyShaderWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::ALL_COMMANDS,
            access_mask: vk::AccessFlags::SHADER_WRITE,
        },
        AccessType::TransferWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::TRANSFER,
            access_mask: vk::AccessFlags::TRANSFER_WRITE,
        },
        AccessType::HostWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::HOST,
            access_mask: vk::AccessFlags::HOST_WRITE,
        },
        AccessType::ColorAttachmentReadWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            access_mask: vk::AccessFlags::COLOR_ATTACHMENT_READ
                | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        },
        AccessType::General => AccessInfo {
            stage_mask: vk::PipelineStageFlags::ALL_COMMANDS,
            access_mask: vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
        },
        AccessType::RayTracingShaderReadSampledImageOrUniformTexelBuffer => AccessInfo {
            stage_mask: vk::PipelineStageFlags::RAY_TRACING_SHADER_KHR,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::RayTracingShaderReadColorInputAttachment => AccessInfo {
            stage_mask: vk::PipelineStageFlags::RAY_TRACING_SHADER_KHR,
            access_mask: vk::AccessFlags::INPUT_ATTACHMENT_READ,
        },
        AccessType::RayTracingShaderReadDepthStencilInputAttachment => AccessInfo {
            stage_mask: vk::PipelineStageFlags::RAY_TRACING_SHADER_KHR,
            access_mask: vk::AccessFlags::INPUT_ATTACHMENT_READ,
        },
        AccessType::RayTracingShaderReadAccelerationStructure => AccessInfo {
            stage_mask: vk::PipelineStageFlags::RAY_TRACING_SHADER_KHR,
            access_mask: vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
        },
        AccessType::RayTracingShaderReadOther => AccessInfo {
            stage_mask: vk::PipelineStageFlags::RAY_TRACING_SHADER_KHR,
            access_mask: vk::AccessFlags::SHADER_READ,
        },
        AccessType::AccelerationStructureBuildWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
            access_mask: vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
        },
        AccessType::AccelerationStructureBuildRead => AccessInfo {
            stage_mask: vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
            access_mask: vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
        },
        AccessType::AccelerationStructureBufferWrite => AccessInfo {
            stage_mask: vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
            access_mask: vk::AccessFlags::TRANSFER_WRITE,
        },
        AccessType::ComputeShaderReadAccelerationStructure => AccessInfo {
            stage_mask: vk::PipelineStageFlags::COMPUTE_SHADER,
            access_mask: vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
        },
    }
}

pub(crate) fn is_write_access(access_type: AccessType) -> bool {
    matches!(
        access_type,
        AccessType::CommandBufferWriteNVX
            | AccessType::VertexShaderWrite
            | AccessType::TessellationControlShaderWrite
            | AccessType::TessellationEvaluationShaderWrite
            | AccessType::GeometryShaderWrite
            | AccessType::FragmentShaderWrite
            | AccessType::ColorAttachmentWrite
            | AccessType::DepthStencilAttachmentWrite
            | AccessType::DepthAttachmentWriteStencilReadOnly
            | AccessType::StencilAttachmentWriteDepthReadOnly
            | AccessType::ComputeShaderWrite
            | AccessType::AnyShaderWrite
            | AccessType::TransferWrite
            | AccessType::HostWrite
            | AccessType::ColorAttachmentReadWrite
            | AccessType::General
            | AccessType::AccelerationStructureBuildWrite
            | AccessType::AccelerationStructureBufferWrite
    )
}
