use std::{marker::PhantomData, sync::Arc};

use parking_lot::{MappedMutexGuard, Mutex, MutexGuard};

use lantir_hal::{
    AccessType, AllocationCreateFlags, Buffer, CopyBufferImageInfo, DescriptorSet, DescriptorSetLayout, ImageBarrier, RenderEngine, Sampler, SamplerInfo, Texture, TextureCreateInfo, UpdateFrequency, vk
};

use crate::resources::*;

pub struct ResourceManager {
    vertex_buffer: Mutex<GpuBuffer<Vertex>>,
    index_buffer: Mutex<GpuBuffer<u32>>,
    items_buffer: Mutex<GpuBuffer<DrawItem>>,
    global_indirect_buffer: Mutex<GpuBuffer<vk::DrawIndexedIndirectCommand>>,

    material_buffer: Mutex<MirroredBuffer<PbrMaterial>>,

    meta_descritor_set_layout: Arc<lantir_hal::DescriptorSetLayout>,
    meta_descriptor_set: DescriptorSet,

    samplers: Mutex<Vec<Sampler>>,

    textures: Mutex<Vec<Texture>>,

    uploaded_meshes: Mutex<Vec<UploadedMesh>>,

    engine: Arc<RenderEngine>,
}

impl ResourceManager {
    pub fn new(engine: Arc<RenderEngine>) -> anyhow::Result<Self> {
        let default_sampler = Sampler::new(
            engine.clone(),
            &SamplerInfo {
                filter: vk::Filter::LINEAR,
            },
        )?;

        // Build buffers first (without Mutex) so we can initialize descriptor sets
        // without locking during construction.
        let vertex_buffer = GpuBuffer::new(engine.clone(), MAX_VERTICES, UpdateFrequency::Static)?;
        let index_buffer = GpuBuffer::new_with_usage(
            engine.clone(),
            MAX_INDICES,
            UpdateFrequency::Static,
            vk::BufferUsageFlags::INDEX_BUFFER,
        )?;
        let items_buffer =
            GpuBuffer::new(engine.clone(), MAX_DRAW_ITEMS, UpdateFrequency::PerFrame)?;
        let global_indirect_buffer = GpuBuffer::new_with_usage(
            engine.clone(),
            MAX_INDIRECT_DRAWS,
            UpdateFrequency::PerFrame,
            vk::BufferUsageFlags::INDIRECT_BUFFER,
        )?;

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
                // samplers
                lantir_hal::DescriptorSetBinding {
                    typ: vk::DescriptorType::SAMPLER,
                    binding: META_BUFFER_BINDING_SAMPLER,
                    stage: vk::ShaderStageFlags::ALL,
                    count: MAX_SAMPLERS as u32,
                },
                // draw items
                lantir_hal::DescriptorSetBinding {
                    typ: vk::DescriptorType::STORAGE_BUFFER,
                    binding: META_BUFFER_BINDING_DRAW_ITEMS,
                    stage: vk::ShaderStageFlags::ALL,
                    count: 1,
                },
            ],
        )?;

        let meta_descriptor_set =
            DescriptorSet::new(engine.clone(), meta_descritor_set_layout.clone())?;

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
            binding: META_BUFFER_BINDING_MATERIAL,
            buffer: material_buffer.buffer(),
            offset: 0,
            size: material_buffer.capacity_bytes(),
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
        });

        meta_descriptor_set.write_buffer(&lantir_hal::WriteBufferInfo {
            binding: META_BUFFER_BINDING_DRAW_ITEMS,
            buffer: items_buffer.buffer(),
            offset: 0,
            size: items_buffer.capacity_bytes(),
            descriptor_type: vk::DescriptorType::STORAGE_BUFFER,
        });

        // Seed sampler array with the default sampler at index 0.
        let samplers = Mutex::new({
            let mut v = Vec::with_capacity(MAX_SAMPLERS);
            v.push(default_sampler);
            v
        });

        // Write the default sampler descriptor (index 0).
        {
            let v = samplers.lock();
            meta_descriptor_set.write_sampler(&lantir_hal::WriteSamplerInfo {
                binding: META_BUFFER_BINDING_SAMPLER,
                sampler: &v[0],
                array_index: 0,
            });
        }

        let uploaded_meshes = Mutex::new(Vec::with_capacity(MAX_MESHES));

        Ok(ResourceManager {
            vertex_buffer: Mutex::new(vertex_buffer),
            index_buffer: Mutex::new(index_buffer),
            material_buffer: Mutex::new(material_buffer),
            meta_descritor_set_layout,
            meta_descriptor_set,
            samplers,
            textures: Mutex::new(Vec::with_capacity(MAX_TEXTURES)),
            items_buffer: Mutex::new(items_buffer),
            global_indirect_buffer: Mutex::new(global_indirect_buffer),
            uploaded_meshes,
            engine,
        })
    }

    pub fn add_sampler(&self, sampler: Sampler) -> anyhow::Result<SamplerHandle> {
        let mut samplers = self.samplers.lock();

        if samplers.len() >= MAX_SAMPLERS {
            anyhow::bail!("Exceeded maximum number of samplers");
        }

        let handle = samplers.len() as SamplerHandle;
        samplers.push(sampler);

        self.meta_descriptor_set
            .write_sampler(&lantir_hal::WriteSamplerInfo {
                binding: META_BUFFER_BINDING_SAMPLER,
                sampler: &samplers[handle as usize],
                array_index: handle,
            });

        Ok(handle)
    }

    fn upload_rgba8_texture(
        &self,
        rgba: &[u8],
        width: u32,
        height: u32,
        format: vk::Format,
    ) -> anyhow::Result<Texture> {
        if width == 0 || height == 0 {
            anyhow::bail!("Invalid texture extent: {}x{}", width, height);
        }

        let staging_size = rgba.len() as u64;
        let staging_buffer = Buffer::new(
            self.engine.clone(),
            &lantir_hal::BufferCreateInfo {
                size: staging_size,
                usage: vk::BufferUsageFlags::TRANSFER_SRC,
                memory_property: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
                update_frequency: UpdateFrequency::Static,
                vma_flags: lantir_hal::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            },
        )?;

        unsafe {
            let staging_map = staging_buffer.map()?;
            std::ptr::copy_nonoverlapping(rgba.as_ptr(), staging_map, rgba.len());
            staging_buffer.unmap();
        }

        let texture = Texture::new(
            self.engine.clone(),
            &TextureCreateInfo {
                image_type: vk::ImageType::TYPE_2D,
                update_frequency: UpdateFrequency::Static,
                format,
                extent: vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                },
                usage: vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
                aspect: vk::ImageAspectFlags::COLOR,
                mip_levels: 1,
            },
        )?;

        self.engine.immediate_submit(|cb| {
            cb.cmd_image_barrier(
                &self.engine,
                &ImageBarrier {
                    previous_accesses: &[AccessType::Nothing],
                    next_accesses: &[AccessType::TransferWrite],
                    previous_layout: vk::ImageLayout::UNDEFINED,
                    next_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    image: &texture,
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                },
            );

            cb.cmd_copy_buffer_to_image(
                &self.engine,
                &CopyBufferImageInfo {
                    buffer: &staging_buffer,
                    image: &texture,
                    image_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    image_aspect_mask: vk::ImageAspectFlags::COLOR,
                    image_extent: vk::Extent3D {
                        width,
                        height,
                        depth: 1,
                    },
                },
            );

            cb.cmd_image_barrier(
                &self.engine,
                &ImageBarrier {
                    previous_accesses: &[AccessType::TransferWrite],
                    next_accesses: &[AccessType::AnyShaderReadSampledImageOrUniformTexelBuffer],
                    previous_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    next_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    image: &texture,
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                },
            );
        })?;

        Ok(texture)
    }

    pub fn add_texture(&self, image: image::DynamicImage) -> anyhow::Result<TextureHandle> {
        let mut textures = self.textures.lock();

        if textures.len() >= MAX_TEXTURES {
            anyhow::bail!("Exceeded maximum number of textures");
        }

        let rgba8 = image.to_rgba8();

        let texture = self.upload_rgba8_texture(
            &rgba8,
            rgba8.width(),
            rgba8.height(),
            vk::Format::R8G8B8A8_UNORM,
        )?;

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

    pub fn add_mesh(&self, mesh: &TriMesh) -> anyhow::Result<MeshHandle> {
        let vertex_offset = {
            let mut vb = self.vertex_buffer.lock();
            vb.add(&mesh.vertices)?
        } as i32;

        let index_offset = {
            let mut ib = self.index_buffer.lock();
            ib.add(&mesh.indices)?
        };

        let mesh = UploadedMesh {
            vertex_offset,
            index_offset,
            index_count: mesh.indices.len() as u32,
        };

        let handle = {
            let mut mb = self.uploaded_meshes.lock();
            mb.push(mesh);
            (mb.len() - 1) as MeshHandle
        };

        Ok(handle)
    }

    pub fn add_material(&self, material: PbrMaterial) -> anyhow::Result<MaterialHandle> {
        let handle = {
            let mut mb = self.material_buffer.lock();
            mb.add(material)?
        };

        Ok(handle)
    }

    pub fn get_mesh(&self, handle: MeshHandle) -> UploadedMesh {
        let mb = self.uploaded_meshes.lock();
        mb[handle as usize]
    }

    pub fn get_material(&self, handle: MaterialHandle) -> PbrMaterial {
        let mb = self.material_buffer.lock();
        mb.get(handle as usize)
    }

    pub fn get_meta_descriptor_set(&self) -> &DescriptorSet {
        &self.meta_descriptor_set
    }

    pub fn get_meta_descriptor_set_layout(&self) -> &Arc<DescriptorSetLayout> {
        &self.meta_descritor_set_layout
    }

    pub fn get_index_buffer(&self) -> MappedMutexGuard<'_, Buffer> {
        let ib = self.index_buffer.lock();
        MutexGuard::map(ib, |ib: &mut GpuBuffer<u32>| &mut ib.buffer)
    }

    pub fn set_draw_items(&self, items: &[DrawItem]) -> anyhow::Result<()> {
        let mut ib = self.items_buffer.lock();

        ib.clear();
        ib.add(items)?;
        Ok(())
    }

    pub fn reset_global_indirect_buffer(&self) -> anyhow::Result<()> {
        let mut db = self.global_indirect_buffer.lock();

        db.clear();
        Ok(())
    }

    pub fn get_global_indirect_buffer(&self) -> MappedMutexGuard<'_, Buffer> {
        let db = self.global_indirect_buffer.lock();

        MutexGuard::map(db, |db: &mut GpuBuffer<vk::DrawIndexedIndirectCommand>| {
            &mut db.buffer
        })
    }

    pub fn add_indirect_draw_commands(
        &self,
        cmd: &[vk::DrawIndexedIndirectCommand],
    ) -> anyhow::Result<u32> {
        let mut db = self.global_indirect_buffer.lock();
        db.add(cmd)
    }
}

struct GpuBuffer<T: Sized> {
    buffer: Buffer,
    num_elems: u32,
    max_elems: usize,
    marker: PhantomData<T>,
    engine: Arc<RenderEngine>,
    update_frequency: UpdateFrequency,
}

impl<T> GpuBuffer<T> {
    pub fn new(
        engine: Arc<RenderEngine>,
        num_elems: usize,
        update_frequency: UpdateFrequency,
    ) -> anyhow::Result<Self> {
        Self::new_with_usage(
            engine,
            num_elems,
            update_frequency,
            vk::BufferUsageFlags::empty(),
        )
    }

    pub fn new_with_usage(
        engine: Arc<RenderEngine>,
        num_elems: usize,
        update_frequency: UpdateFrequency,
        extra_usage: vk::BufferUsageFlags,
    ) -> anyhow::Result<Self> {
        let alloc_size = std::mem::size_of::<T>() * num_elems;

        let buffer = Buffer::new(
            engine.clone(),
            &lantir_hal::BufferCreateInfo {
                size: alloc_size as u64,
                usage: vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | extra_usage,
                memory_property: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                update_frequency: update_frequency,
                vma_flags: AllocationCreateFlags::empty(),
            },
        )?;

        Ok(GpuBuffer {
            buffer,
            num_elems: 0,
            max_elems: num_elems,
            marker: PhantomData,
            engine,
            update_frequency,
        })
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn capacity_bytes(&self) -> u64 {
        (self.max_elems * std::mem::size_of::<T>()) as u64
    }

    pub fn add(&mut self, data: &[T]) -> anyhow::Result<u32> {
        if data.is_empty() {
            // Avoid creating 0-sized staging buffers (invalid in Vulkan).
            return Ok(self.num_elems);
        }

        if self.num_elems as usize + data.len() > self.max_elems {
            anyhow::bail!("GpuBuffer overflow");
        }

        let offset = self.num_elems as u64 * std::mem::size_of::<T>() as u64;
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

        let res = self.num_elems;
        self.num_elems += data.len() as u32;
        Ok(res)
    }

    pub fn clear(&mut self) {
        assert_eq!(self.update_frequency, UpdateFrequency::PerFrame);
        self.num_elems = 0;
    }
}

struct MirroredBuffer<T: Sized + Copy> {
    buffer: GpuBuffer<T>,
    data: Vec<T>,
}

impl<T: Sized + Copy> MirroredBuffer<T> {
    pub fn new(engine: Arc<RenderEngine>, num_elems: usize) -> anyhow::Result<Self> {
        let buffer = GpuBuffer::new(engine, num_elems, UpdateFrequency::Static)?;
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

    pub fn get(&self, index: usize) -> T {
        self.data[index]
    }

    pub fn buffer(&self) -> &Buffer {
        self.buffer.buffer()
    }

    pub fn capacity_bytes(&self) -> u64 {
        self.buffer.capacity_bytes()
    }
}
