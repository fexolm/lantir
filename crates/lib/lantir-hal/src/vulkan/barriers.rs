use crate::barriers::{AccessType, AspectMask, ImageBarrier, ImageLayout};
use crate::vulkan::device::VulkanDevice;
use crate::vulkan::VulkanImage;
use crate::Image;
use ash::vk;

pub fn get_image_memory_barrier<'a>(
    device: &VulkanDevice,
    barrier: &ImageBarrier<'a>,
) -> (
    vk::PipelineStageFlags,
    vk::PipelineStageFlags,
    vk::ImageMemoryBarrier<'a>,
) {
    let mut src_stages = vk::PipelineStageFlags::empty();
    let mut dst_stages = vk::PipelineStageFlags::empty();

    let mut image_barrier = vk::ImageMemoryBarrier {
        src_queue_family_index: device.universal_family,
        dst_queue_family_index: device.universal_family,
        image: barrier.image.get_imp().get_image(),
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

        image_barrier.old_layout = convert_layout(barrier.previous_layout);
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

        image_barrier.new_layout = convert_layout(barrier.next_layout);
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

fn convert_aspect_mask(aspect_mask: AspectMask) -> vk::ImageAspectFlags {
    match aspect_mask {
        AspectMask::Color => vk::ImageAspectFlags::COLOR,
        AspectMask::Depth => vk::ImageAspectFlags::DEPTH,
        AspectMask::Stencil => vk::ImageAspectFlags::STENCIL,
        AspectMask::Metadata => vk::ImageAspectFlags::METADATA,
    }
}

pub fn convert_layout(layout: ImageLayout) -> vk::ImageLayout {
    match layout {
        ImageLayout::Undefined => vk::ImageLayout::UNDEFINED,
        ImageLayout::General => vk::ImageLayout::GENERAL,
        ImageLayout::ShaderReadOnlyOptimal => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        ImageLayout::DepthStencilReadOnlyOptimal => {
            vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL
        }
        ImageLayout::DepthStencilAttachmentOptimal => {
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        }
        ImageLayout::DepthAttachmentStencilReadOnlyOptimal => {
            vk::ImageLayout::DEPTH_ATTACHMENT_STENCIL_READ_ONLY_OPTIMAL
        }
        ImageLayout::DepthReadOnlyStencilAttachmentOptimal => {
            vk::ImageLayout::DEPTH_READ_ONLY_STENCIL_ATTACHMENT_OPTIMAL
        }
        ImageLayout::ColorAttachmentOptimal => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        ImageLayout::TransferSrcOptimal => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        ImageLayout::TransferDstOptimal => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        ImageLayout::PresentSrc => vk::ImageLayout::PRESENT_SRC_KHR,
    }
}

pub fn make_subresource_range(aspect_mask: AspectMask) -> vk::ImageSubresourceRange {
    let aspect_mask = convert_aspect_mask(aspect_mask);

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
    )
}
