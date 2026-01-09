use std::sync::Arc;

use lantir_hal::{
    vk, AllocationCreateFlags, Buffer, CommandBuffer, DescriptorSet, RenderEngine, UpdateFrequency,
};

use crate::material::{GPUDrawPushConstants, MetallicRoughnessMatInstance};

pub struct Mesh {
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    index_count: u32,
    pub vertices: Vec<Vertex>,
}

pub struct RenderObject {
    pub mesh: Mesh,
    pub material: MetallicRoughnessMatInstance,
    pub transform: glam::Mat4,
}

impl RenderObject {
    pub fn draw(&self, engine: &RenderEngine, cb: &CommandBuffer, scene_set: &DescriptorSet) {
        self.material.bind(engine, cb, scene_set);

        let push_constants = GPUDrawPushConstants {
            render_matrix: self.transform,
            vert_address: self.mesh.vertex_buffer.get_device_address(),
        };

        self.material.push_constants(engine, &push_constants, cb);
        cb.cmd_bind_index_buffer(&engine, &self.mesh.index_buffer, vk::IndexType::UINT32);

        cb.cmd_draw_indexed(&engine, self.mesh.index_count, 1);
    }
}

pub fn load_mesh(
    engine: Arc<RenderEngine>,
    vertices: &[Vertex],
    indices: &[u32],
) -> anyhow::Result<Mesh> {
    let vertex_buffer_size = (size_of::<Vertex>() * vertices.len()) as vk::DeviceSize;

    let vertex_buffer = Buffer::new(
        engine.clone(),
        &lantir_hal::BufferCreateInfo {
            size: vertex_buffer_size,
            usage: vk::BufferUsageFlags::VERTEX_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            memory_property: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            update_frequency: UpdateFrequency::Static,
            vma_flags: AllocationCreateFlags::empty(),
        },
    )?;

    let index_buffer_size = (std::mem::size_of::<u32>() * indices.len()) as vk::DeviceSize;

    let index_buffer = Buffer::new(
        engine.clone(),
        &lantir_hal::BufferCreateInfo {
            size: index_buffer_size,
            usage: vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            memory_property: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            update_frequency: UpdateFrequency::Static,
            vma_flags: AllocationCreateFlags::empty(),
        },
    )?;

    let staging_buffer = Buffer::new(
        engine.clone(),
        &lantir_hal::BufferCreateInfo {
            size: (std::mem::size_of::<Vertex>() * vertices.len()
                + std::mem::size_of::<u32>() * indices.len()) as u64,
            usage: vk::BufferUsageFlags::TRANSFER_SRC,
            memory_property: vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT,
            update_frequency: UpdateFrequency::Static,
            vma_flags: AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
        },
    )?;

    unsafe {
        let staging_buffer_map = staging_buffer.map()?;

        std::ptr::copy_nonoverlapping(
            vertices.as_ptr() as *const u8,
            staging_buffer_map,
            vertex_buffer_size as usize,
        );

        std::ptr::copy_nonoverlapping(
            indices.as_ptr() as *const u8,
            staging_buffer_map.add(vertex_buffer_size as usize),
            index_buffer_size as usize,
        );

        staging_buffer.unmap();
    }

    engine.immediate_submit(|cb| {
        cb.cmd_copy_buffer(
            &engine,
            &staging_buffer,
            &vertex_buffer,
            vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: vertex_buffer_size,
            },
        );
        cb.cmd_copy_buffer(
            &engine,
            &staging_buffer,
            &index_buffer,
            vk::BufferCopy {
                src_offset: vertex_buffer_size,
                dst_offset: 0,
                size: index_buffer_size,
            },
        );
    })?;

    Ok(Mesh {
        index_buffer,
        vertex_buffer,
        index_count: indices.len() as u32,
        vertices: Vec::from(vertices),
    })
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct Vertex {
    pub position: glam::Vec3,
    pub normal: glam::Vec3,
    pub color: glam::Vec4,
    pub uv: glam::Vec2,
}
