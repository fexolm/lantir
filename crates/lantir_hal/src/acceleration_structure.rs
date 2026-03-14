use crate::resource::{DeferDrop, Resource};
use crate::{Buffer, BufferCreateInfo, RenderEngine, UpdateFrequency};
use ash::vk;
use std::sync::Arc;
use vk_mem::AllocationCreateFlags;

/// A Vulkan acceleration structure (BLAS or TLAS) managed by the engine.
pub struct AccelerationStructureData {
    pub(crate) handle: vk::AccelerationStructureKHR,
    /// Buffer backing the acceleration structure storage.
    _backing_buffer: Buffer,
    pub(crate) device_address: u64,
}

impl DeferDrop for AccelerationStructureData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            engine
                .acceleration_structure_loader
                .destroy_acceleration_structure(self.handle, None);
        }
        // _backing_buffer drops here via its own DeferDrop impl.
    }
}

pub type AccelerationStructure = Resource<AccelerationStructureData>;

impl AccelerationStructure {
    pub fn get_device_address(&self) -> u64 {
        self.get_handle().device_address
    }

    pub fn get_raw_handle(&self) -> vk::AccelerationStructureKHR {
        self.get_handle().handle
    }
}

/// Instance data for a TLAS entry, matching `VkAccelerationStructureInstanceKHR` layout exactly.
/// The layout is: 12 floats (row-major 3×4 transform), then two packed u32s, then a u64 BLAS address.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct TlasInstance {
    /// Row-major 3×4 affine transform (rows 0-2 of the model matrix).
    pub transform: [f32; 12],
    /// bits [23:0] = instance_custom_index, bits [31:24] = mask.
    pub instance_custom_index_and_mask: u32,
    /// bits [23:0] = SBT record offset, bits [31:24] = VkGeometryInstanceFlagsKHR.
    pub instance_shader_binding_table_record_offset_and_flags: u32,
    /// Device address of the BLAS.
    pub acceleration_structure_reference: u64,
}

impl TlasInstance {
    /// Construct a TLAS instance from a BLAS device address and a model transform.
    pub fn new(blas_address: u64, model: glam::Mat4, instance_custom_index: u32, mask: u8) -> Self {
        // glam Mat4 is column-major.  We extract the top 3 rows as row-major 3×4.
        let cols = model.to_cols_array_2d(); // cols[col][row]
        let t = [
            cols[0][0], cols[1][0], cols[2][0], cols[3][0], // row 0
            cols[0][1], cols[1][1], cols[2][1], cols[3][1], // row 1
            cols[0][2], cols[1][2], cols[2][2], cols[3][2], // row 2
        ];
        TlasInstance {
            transform: t,
            instance_custom_index_and_mask: (instance_custom_index & 0x00FF_FFFF)
                | ((mask as u32) << 24),
            instance_shader_binding_table_record_offset_and_flags: 0,
            acceleration_structure_reference: blas_address,
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn align_up(value: vk::DeviceSize, alignment: vk::DeviceSize) -> vk::DeviceSize {
    (value + alignment - 1) & !(alignment - 1)
}

fn create_as_buffer(
    engine: Arc<RenderEngine>,
    size: vk::DeviceSize,
    extra_usage: vk::BufferUsageFlags,
) -> anyhow::Result<Buffer> {
    Buffer::new(
        engine,
        &BufferCreateInfo {
            size,
            usage: extra_usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            memory_property: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            update_frequency: UpdateFrequency::Static,
            vma_flags: AllocationCreateFlags::empty(),
        },
    )
}

// ---------------------------------------------------------------------------
// BLAS build
// ---------------------------------------------------------------------------

/// Build a BLAS from a triangle mesh that is already resident on the device.
///
/// - `vertex_buffer_address`: device address of the vertex buffer; positions
///   are at offset 0, format `R32G32B32_SFLOAT`, stride `vertex_stride`.
/// - `vertex_count`: number of vertices (max_vertex for the build).
/// - `index_buffer_address`: device address of the index buffer (u32 indices).
/// - `triangle_count`: number of triangles (`index_count / 3`).
pub fn build_blas(
    engine: &Arc<RenderEngine>,
    vertex_buffer_address: u64,
    vertex_count: u32,
    vertex_stride: u64,
    index_buffer_address: u64,
    triangle_count: u32,
) -> anyhow::Result<AccelerationStructure> {
    let loader = &engine.acceleration_structure_loader;

    let geometry = vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
        .flags(vk::GeometryFlagsKHR::OPAQUE)
        .geometry(vk::AccelerationStructureGeometryDataKHR {
            triangles: vk::AccelerationStructureGeometryTrianglesDataKHR::default()
                .vertex_format(vk::Format::R32G32B32_SFLOAT)
                .vertex_data(vk::DeviceOrHostAddressConstKHR {
                    device_address: vertex_buffer_address,
                })
                .vertex_stride(vertex_stride)
                .max_vertex(vertex_count.saturating_sub(1))
                .index_type(vk::IndexType::UINT32)
                .index_data(vk::DeviceOrHostAddressConstKHR {
                    device_address: index_buffer_address,
                })
                .transform_data(vk::DeviceOrHostAddressConstKHR::default()),
        });

    let geometries = [geometry];
    let range_info = vk::AccelerationStructureBuildRangeInfoKHR::default()
        .primitive_count(triangle_count)
        .primitive_offset(0)
        .first_vertex(0)
        .transform_offset(0);

    let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
        .geometries(&geometries);

    let primitive_counts = [triangle_count];
    let mut size_info = vk::AccelerationStructureBuildSizesInfoKHR::default();
    unsafe {
        loader.get_acceleration_structure_build_sizes(
            vk::AccelerationStructureBuildTypeKHR::DEVICE,
            &build_info,
            &primitive_counts,
            &mut size_info,
        );
    }

    let as_buffer = create_as_buffer(
        engine.clone(),
        size_info.acceleration_structure_size,
        vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR,
    )?;
    let as_buffer_raw = as_buffer.get_buffer();

    let as_create_info = vk::AccelerationStructureCreateInfoKHR::default()
        .buffer(as_buffer_raw)
        .size(size_info.acceleration_structure_size)
        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL);

    let handle = unsafe { loader.create_acceleration_structure(&as_create_info, None)? };

    let scratch_buffer = create_as_buffer(
        engine.clone(),
        align_up(size_info.build_scratch_size, 256),
        vk::BufferUsageFlags::STORAGE_BUFFER,
    )?;
    let scratch_address = scratch_buffer.get_device_address();

    let as_addr_info =
        vk::AccelerationStructureDeviceAddressInfoKHR::default().acceleration_structure(handle);
    let device_address = unsafe { loader.get_acceleration_structure_device_address(&as_addr_info) };

    let build_info_with_dst = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL)
        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
        .geometries(&geometries)
        .dst_acceleration_structure(handle)
        .scratch_data(vk::DeviceOrHostAddressKHR {
            device_address: scratch_address,
        });

    let range_infos_inner = [range_info];
    let range_infos: Vec<&[vk::AccelerationStructureBuildRangeInfoKHR]> = vec![&range_infos_inner];
    let build_geometry_infos = [build_info_with_dst];

    engine.immediate_submit(|cb| unsafe {
        loader.cmd_build_acceleration_structures(
            cb.command_buffer,
            &build_geometry_infos,
            &range_infos,
        );
    })?;

    Ok(Resource::make(
        engine.clone(),
        AccelerationStructureData {
            handle,
            _backing_buffer: as_buffer,
            device_address,
        },
    ))
}

// ---------------------------------------------------------------------------
// TLAS build
// ---------------------------------------------------------------------------

/// Build a TLAS from a list of BLAS instances.
pub fn build_tlas(
    engine: &Arc<RenderEngine>,
    instances: &[TlasInstance],
) -> anyhow::Result<AccelerationStructure> {
    if instances.is_empty() {
        anyhow::bail!("build_tlas: instances slice is empty");
    }

    let loader = &engine.acceleration_structure_loader;
    let instance_data_size =
        (std::mem::size_of::<TlasInstance>() * instances.len()) as vk::DeviceSize;

    // Stage instance data into a host-visible buffer, then copy to device-local.
    let staging = Buffer::new(
        engine.clone(),
        &BufferCreateInfo {
            size: instance_data_size,
            usage: vk::BufferUsageFlags::TRANSFER_SRC,
            memory_property: vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT,
            update_frequency: UpdateFrequency::Static,
            vma_flags: AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
        },
    )?;

    unsafe {
        let ptr = staging.map()?;
        std::ptr::copy_nonoverlapping(
            instances.as_ptr() as *const u8,
            ptr,
            instance_data_size as usize,
        );
        staging.unmap();
    }

    let instance_buffer = Buffer::new(
        engine.clone(),
        &BufferCreateInfo {
            size: instance_data_size,
            usage: vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
            memory_property: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            update_frequency: UpdateFrequency::Static,
            vma_flags: AllocationCreateFlags::empty(),
        },
    )?;

    engine.immediate_submit(|cb| {
        cb.cmd_copy_buffer(
            engine,
            &staging,
            &instance_buffer,
            vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: instance_data_size,
            },
        );
    })?;

    let instance_buffer_address = instance_buffer.get_device_address();
    let instance_count = instances.len() as u32;

    let geometry = vk::AccelerationStructureGeometryKHR::default()
        .geometry_type(vk::GeometryTypeKHR::INSTANCES)
        .flags(vk::GeometryFlagsKHR::OPAQUE)
        .geometry(vk::AccelerationStructureGeometryDataKHR {
            instances: vk::AccelerationStructureGeometryInstancesDataKHR::default()
                .array_of_pointers(false)
                .data(vk::DeviceOrHostAddressConstKHR {
                    device_address: instance_buffer_address,
                }),
        });

    let geometries = [geometry];

    let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
        .geometries(&geometries);

    let primitive_counts = [instance_count];
    let mut size_info = vk::AccelerationStructureBuildSizesInfoKHR::default();
    unsafe {
        loader.get_acceleration_structure_build_sizes(
            vk::AccelerationStructureBuildTypeKHR::DEVICE,
            &build_info,
            &primitive_counts,
            &mut size_info,
        );
    }

    let as_buffer = create_as_buffer(
        engine.clone(),
        size_info.acceleration_structure_size,
        vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR,
    )?;
    let as_buffer_raw = as_buffer.get_buffer();

    let as_create_info = vk::AccelerationStructureCreateInfoKHR::default()
        .buffer(as_buffer_raw)
        .size(size_info.acceleration_structure_size)
        .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL);

    let handle = unsafe { loader.create_acceleration_structure(&as_create_info, None)? };

    let scratch_buffer = create_as_buffer(
        engine.clone(),
        align_up(size_info.build_scratch_size, 256),
        vk::BufferUsageFlags::STORAGE_BUFFER,
    )?;
    let scratch_address = scratch_buffer.get_device_address();

    let as_addr_info =
        vk::AccelerationStructureDeviceAddressInfoKHR::default().acceleration_structure(handle);
    let device_address = unsafe { loader.get_acceleration_structure_device_address(&as_addr_info) };

    let build_info_with_dst = vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
        .geometries(&geometries)
        .dst_acceleration_structure(handle)
        .scratch_data(vk::DeviceOrHostAddressKHR {
            device_address: scratch_address,
        });

    let range_info = vk::AccelerationStructureBuildRangeInfoKHR::default()
        .primitive_count(instance_count)
        .primitive_offset(0)
        .first_vertex(0)
        .transform_offset(0);

    let range_infos_inner = [range_info];
    let range_infos: Vec<&[vk::AccelerationStructureBuildRangeInfoKHR]> = vec![&range_infos_inner];
    let build_geometry_infos = [build_info_with_dst];

    engine.immediate_submit(|cb| unsafe {
        loader.cmd_build_acceleration_structures(
            cb.command_buffer,
            &build_geometry_infos,
            &range_infos,
        );
    })?;

    Ok(Resource::make(
        engine.clone(),
        AccelerationStructureData {
            handle,
            _backing_buffer: as_buffer,
            device_address,
        },
    ))
}
