use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use lantir_hal::{
    AllocationCreateFlags, Buffer, DescriptorSet, DescriptorSetLayout, RenderEngine, Texture,
    UpdateFrequency, vk,
};

use crate::resources::{
    GpuMesh, MAX_INDICES, MAX_MATERIALS, MAX_MESHES, MAX_TEXTURES, MAX_VERTICES,
    META_BUFFER_BINDING_MATERIAL, META_BUFFER_BINDING_MESH, META_BUFFER_BINDING_TEXTURE,
    META_BUFFER_BINDING_VERTEX, MaterialHandle, MeshHandle, PbrMaterial, TextureHandle, TriMesh,
    Vertex,
};

pub struct ResourceManager {
    vertex_buffer: Mutex<GpuBuffer<Vertex>>,
    index_buffer: Mutex<GpuBuffer<u32>>,

    mesh_buffer: Mutex<MirroredBuffer<GpuMesh>>,
    material_buffer: Mutex<MirroredBuffer<PbrMaterial>>,

    meta_descritor_set_layout: Arc<lantir_hal::DescriptorSetLayout>,
    meta_descriptor_set: DescriptorSet,

    textures: Mutex<Vec<Texture>>,
}

impl ResourceManager {
    pub fn new(engine: Arc<RenderEngine>) -> anyhow::Result<Self> {
        // Build buffers first (without Mutex) so we can initialize descriptor sets
        // without locking during construction.
        let vertex_buffer = GpuBuffer::new(engine.clone(), MAX_VERTICES)?;
        let index_buffer = GpuBuffer::new(engine.clone(), MAX_INDICES)?;
        let mesh_buffer = MirroredBuffer::new(engine.clone(), MAX_MESHES)?;
        let material_buffer = MirroredBuffer::new(engine.clone(), MAX_MATERIALS)?;

        let meta_descritor_set_layout = DescriptorSetLayout::new(
            engine.clone(),
            &[
                // vertex buffer
                lantir_hal::DescriptorSetBinding {
                    typ: vk::DescriptorType::STORAGE_BUFFER,
                    binding: META_BUFFER_BINDING_VERTEX,
                    stage: vk::ShaderStageFlags::ALL,
                    count: 1,
                },
                // mesh buffer
                lantir_hal::DescriptorSetBinding {
                    typ: vk::DescriptorType::STORAGE_BUFFER,
                    binding: META_BUFFER_BINDING_MESH,
                    stage: vk::ShaderStageFlags::ALL,
                    count: 1,
                },
                // material buffer
                lantir_hal::DescriptorSetBinding {
                    typ: vk::DescriptorType::STORAGE_BUFFER,
                    binding: META_BUFFER_BINDING_MATERIAL,
                    stage: vk::ShaderStageFlags::ALL,
                    count: 1,
                },
                // textures
                lantir_hal::DescriptorSetBinding {
                    typ: vk::DescriptorType::SAMPLED_IMAGE,
                    binding: META_BUFFER_BINDING_TEXTURE,
                    stage: vk::ShaderStageFlags::ALL,
                    count: MAX_TEXTURES as u32,
                },
            ],
        )?;

        let meta_descriptor_set = DescriptorSet::new(
            engine.clone(),
            meta_descritor_set_layout.clone(),
            UpdateFrequency::Static,
        )?;

        // Bind meta buffers once (static descriptor set). Without this, shaders reading these
        // bindings will see uninitialized descriptors.
        meta_descriptor_set.write_buffer(&lantir_hal::WriteBufferInfo {
            binding: META_BUFFER_BINDING_VERTEX,
            buffer: vertex_buffer.buffer(),
            offset: 0,
            size: vertex_buffer.capacity_bytes(),
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
        });

        meta_descriptor_set.write_buffer(&lantir_hal::WriteBufferInfo {
            binding: META_BUFFER_BINDING_MESH,
            buffer: mesh_buffer.buffer(),
            offset: 0,
            size: mesh_buffer.capacity_bytes(),
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
        });

        meta_descriptor_set.write_buffer(&lantir_hal::WriteBufferInfo {
            binding: META_BUFFER_BINDING_MATERIAL,
            buffer: material_buffer.buffer(),
            offset: 0,
            size: material_buffer.capacity_bytes(),
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
        });

        Ok(ResourceManager {
            vertex_buffer: Mutex::new(vertex_buffer),
            index_buffer: Mutex::new(index_buffer),
            mesh_buffer: Mutex::new(mesh_buffer),
            material_buffer: Mutex::new(material_buffer),
            meta_descritor_set_layout,
            meta_descriptor_set,
            textures: Mutex::new(Vec::with_capacity(MAX_TEXTURES)),
        })
    }

    pub fn add_texture(&self, texture: Texture) -> anyhow::Result<TextureHandle> {
        let mut textures = self
            .textures
            .lock()
            .map_err(|_| anyhow::anyhow!("textures mutex poisoned"))?;

        if textures.len() >= MAX_TEXTURES {
            anyhow::bail!("Exceeded maximum number of textures");
        }

        let handle = textures.len() as TextureHandle;
        textures.push(texture);

        self.meta_descriptor_set
            .write_image(&lantir_hal::WriteImageInfo {
                binding: META_BUFFER_BINDING_TEXTURE,
                image: &textures[handle as usize],
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
                sampler: None,
                array_index: handle,
            });
        Ok(handle)
    }

    pub fn add_mesh(&self, mesh: &TriMesh, material: MaterialHandle) -> anyhow::Result<MeshHandle> {
        let vertex_offset = {
            let mut vb = self
                .vertex_buffer
                .lock()
                .map_err(|_| anyhow::anyhow!("vertex_buffer mutex poisoned"))?;
            vb.add(&mesh.vertices)?
        };

        let index_offset = {
            let mut ib = self
                .index_buffer
                .lock()
                .map_err(|_| anyhow::anyhow!("index_buffer mutex poisoned"))?;
            ib.add(&mesh.indices)?
        };

        let mesh = GpuMesh {
            index_offset,
            vertex_offset,
            material,
        };

        let handle = {
            let mut mb = self
                .mesh_buffer
                .lock()
                .map_err(|_| anyhow::anyhow!("mesh_buffer mutex poisoned"))?;
            mb.add(mesh)?
        };

        Ok(handle)
    }

    pub fn add_material(&self, material: PbrMaterial) -> anyhow::Result<MaterialHandle> {
        let handle = {
            let mut mb = self
                .material_buffer
                .lock()
                .map_err(|_| anyhow::anyhow!("material_buffer mutex poisoned"))?;
            mb.add(material)?
        };

        Ok(handle)
    }

    pub fn get_meta_descriptor_set(&self) -> &DescriptorSet {
        &self.meta_descriptor_set
    }

    pub fn get_meta_descriptor_set_layout(&self) -> &Arc<DescriptorSetLayout> {
        &self.meta_descritor_set_layout
    }
}

struct GpuBuffer<T: Sized> {
    buffer: Buffer,
    num_elems: u64,
    max_elems: usize,
    marker: PhantomData<T>,
    engine: Arc<RenderEngine>,
}

impl<T> GpuBuffer<T> {
    pub fn new(engine: Arc<RenderEngine>, num_elems: usize) -> anyhow::Result<Self> {
        let alloc_size = std::mem::size_of::<T>() * num_elems;

        let buffer = Buffer::new(
            engine.clone(),
            &lantir_hal::BufferCreateInfo {
                size: alloc_size as u64,
                usage: vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                memory_property: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                update_frequency: UpdateFrequency::Static,
                vma_flags: AllocationCreateFlags::empty(),
            },
        )?;

        Ok(GpuBuffer {
            buffer,
            num_elems: 0,
            max_elems: num_elems,
            marker: PhantomData,
            engine,
        })
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn capacity_bytes(&self) -> u64 {
        (self.max_elems * std::mem::size_of::<T>()) as u64
    }

    pub fn add(&mut self, data: &[T]) -> anyhow::Result<u64> {
        if self.num_elems as usize + data.len() > self.max_elems {
            anyhow::bail!("GpuBuffer overflow");
        }

        let offset = self.num_elems * size_of::<T>() as u64;
        let staging_size = (data.len() * std::mem::size_of::<T>()) as u64;

        let staging_buffer = Buffer::new(
            self.engine.clone(),
            &lantir_hal::BufferCreateInfo {
                size: staging_size,
                usage: vk::BufferUsageFlags::TRANSFER_SRC,
                memory_property: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
                update_frequency: UpdateFrequency::Static,
                vma_flags: AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            },
        )?;

        unsafe {
            let staging_map = staging_buffer.map()?;
            std::ptr::copy_nonoverlapping(
                data.as_ptr() as *const u8,
                staging_map,
                staging_size as usize,
            );
            staging_buffer.unmap();
        }

        self.engine.immediate_submit(|cb| {
            cb.cmd_copy_buffer(
                &self.engine,
                &staging_buffer,
                &self.buffer,
                vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: offset,
                    size: staging_size,
                },
            );
        })?;

        self.num_elems += data.len() as u64;

        Ok(offset)
    }
}

struct MirroredBuffer<T: Sized + Copy> {
    buffer: GpuBuffer<T>,
    data: Vec<T>,
}

impl<T: Sized + Copy> MirroredBuffer<T> {
    pub fn new(engine: Arc<RenderEngine>, num_elems: usize) -> anyhow::Result<Self> {
        let buffer = GpuBuffer::new(engine, num_elems)?;
        let data = Vec::with_capacity(num_elems);
        Ok(MirroredBuffer { buffer, data })
    }

    pub fn add(&mut self, val: T) -> anyhow::Result<u64>
    where
        T: Copy,
    {
        let id = self.data.len();
        self.buffer.add(&[val])?;
        self.data.push(val);
        Ok(id as u64)
    }

    pub fn buffer(&self) -> &Buffer {
        self.buffer.buffer()
    }

    pub fn capacity_bytes(&self) -> u64 {
        self.buffer.capacity_bytes()
    }
}
