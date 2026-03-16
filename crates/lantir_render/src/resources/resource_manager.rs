use std::{marker::PhantomData, sync::Arc};

use anyhow::Context;

use parking_lot::{MappedMutexGuard, Mutex, MutexGuard};

use lantir_hal::acceleration_structure::{
    AccelerationStructure, TlasInstance, build_blas, build_tlas,
};
use lantir_hal::{
    AccessType, AllocationCreateFlags, Buffer, CopyBufferImageInfo, DescriptorSet,
    DescriptorSetLayout, ImageBarrier, RenderEngine, Sampler, SamplerInfo, Texture,
    TextureCreateInfo, UpdateFrequency, vk,
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

    skybox_buffer: Buffer,

    samplers: Mutex<Vec<Sampler>>,
    textures: Mutex<Vec<Texture>>,
    uploaded_meshes: Mutex<Vec<UploadedMesh>>,

    // Per-frame TLAS: rebuilt every frame from current draw_items.
    // One slot per frame-in-flight; the slot being rendered always owns a valid TLAS.
    tlas: Vec<Mutex<Option<AccelerationStructure>>>,

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

        let skybox_buffer = Buffer::new(
            engine.clone(),
            &lantir_hal::BufferCreateInfo {
                size: std::mem::size_of::<Skybox>() as u64,
                usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
                memory_property: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
                update_frequency: UpdateFrequency::Static,
                vma_flags: AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            },
        )?;

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
                // skybox
                lantir_hal::DescriptorSetBinding {
                    typ: vk::DescriptorType::UNIFORM_BUFFER,
                    binding: META_BUFFER_BINDING_SKYBOX,
                    stage: vk::ShaderStageFlags::ALL,
                    count: 1,
                },
                // index buffer (for RT hit shader normal interpolation)
                lantir_hal::DescriptorSetBinding {
                    typ: vk::DescriptorType::STORAGE_BUFFER,
                    binding: META_BUFFER_BINDING_INDEX,
                    stage: vk::ShaderStageFlags::ALL,
                    count: 1,
                },
                // TLAS (per-frame, rebuilt from current draw_items each frame)
                lantir_hal::DescriptorSetBinding {
                    typ: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                    binding: META_BUFFER_BINDING_TLAS,
                    stage: vk::ShaderStageFlags::RAYGEN_KHR | vk::ShaderStageFlags::CLOSEST_HIT_KHR,
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

        meta_descriptor_set.write_buffer(&lantir_hal::WriteBufferInfo {
            binding: META_BUFFER_BINDING_SKYBOX,
            buffer: &skybox_buffer,
            offset: 0,
            size: std::mem::size_of::<Skybox>() as u64,
            descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
        });

        meta_descriptor_set.write_buffer(&lantir_hal::WriteBufferInfo {
            binding: META_BUFFER_BINDING_INDEX,
            buffer: index_buffer.buffer(),
            offset: 0,
            size: index_buffer.capacity_bytes(),
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

        let frames_in_flight = engine.frames_in_flight();
        let tlas = (0..frames_in_flight).map(|_| Mutex::new(None)).collect();

        Ok(ResourceManager {
            vertex_buffer: Mutex::new(vertex_buffer),
            index_buffer: Mutex::new(index_buffer),
            material_buffer: Mutex::new(material_buffer),
            meta_descritor_set_layout,
            meta_descriptor_set,
            skybox_buffer,
            samplers,
            textures: Mutex::new(Vec::with_capacity(MAX_TEXTURES)),
            items_buffer: Mutex::new(items_buffer),
            global_indirect_buffer: Mutex::new(global_indirect_buffer),
            uploaded_meshes,
            tlas,
            engine,
        })
    }

    fn write_skybox(&self, skybox: &Skybox) -> anyhow::Result<()> {
        unsafe {
            let ptr = self.skybox_buffer.map()?;
            std::ptr::copy_nonoverlapping(
                skybox as *const Skybox as *const u8,
                ptr,
                std::mem::size_of::<Skybox>(),
            );
            self.skybox_buffer.unmap();
        }
        Ok(())
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

    fn upload_texture_from_bytes(
        &self,
        bytes: &[u8],
        width: u32,
        height: u32,
        format: vk::Format,
    ) -> anyhow::Result<Texture> {
        if width == 0 || height == 0 {
            anyhow::bail!("Invalid texture extent: {}x{}", width, height);
        }

        let staging_size = bytes.len() as u64;
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
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), staging_map, bytes.len());
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

    fn upload_rgba16f_texture(
        &self,
        rgba16f: &[half::f16],
        width: u32,
        height: u32,
    ) -> anyhow::Result<Texture> {
        let mut bytes: Vec<u8> = Vec::with_capacity(rgba16f.len() * 2);
        for v in rgba16f {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        self.upload_texture_from_bytes(&bytes, width, height, vk::Format::R16G16B16A16_SFLOAT)
    }

    pub fn set_skybox_image(
        &self,
        image: image::DynamicImage,
        irradiance: image::DynamicImage,
        prefiltered: image::DynamicImage,
        brdf_lut: image::DynamicImage,
        exposure: f32,
        ambient_floor: f32,
    ) -> anyhow::Result<()> {
        self.write_skybox(&Skybox {
            tex: self.add_hdr_texture(image)? as u32,
            sampler: DEFAULT_SAMPLER_HANDLE as u32,
            irradiance_tex: self.add_hdr_texture(irradiance)? as u32,
            irradiance_sampler: DEFAULT_SAMPLER_HANDLE as u32,
            prefiltered_tex: self.add_hdr_texture(prefiltered)? as u32,
            prefiltered_sampler: DEFAULT_SAMPLER_HANDLE as u32,
            brdf_lut_tex: self.add_hdr_texture(brdf_lut)? as u32,
            brdf_lut_sampler: DEFAULT_SAMPLER_HANDLE as u32,
            exposure,
            ambient_floor,
        })
    }

    fn add_ldr_texture(
        &self,
        image: image::DynamicImage,
        format: vk::Format,
    ) -> anyhow::Result<TextureHandle> {
        let mut textures = self.textures.lock();

        if textures.len() >= MAX_TEXTURES {
            anyhow::bail!("Exceeded maximum number of textures");
        }

        let rgba8 = image.to_rgba8();

        let texture =
            self.upload_texture_from_bytes(&rgba8, rgba8.width(), rgba8.height(), format)?;

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

    pub fn add_texture(&self, image: image::DynamicImage) -> anyhow::Result<TextureHandle> {
        self.add_texture_linear(image)
    }

    pub fn add_texture_linear(&self, image: image::DynamicImage) -> anyhow::Result<TextureHandle> {
        self.add_ldr_texture(image, vk::Format::R8G8B8A8_UNORM)
    }

    pub fn add_texture_srgb(&self, image: image::DynamicImage) -> anyhow::Result<TextureHandle> {
        self.add_ldr_texture(image, vk::Format::R8G8B8A8_SRGB)
    }

    pub fn add_hdr_texture(&self, image: image::DynamicImage) -> anyhow::Result<TextureHandle> {
        let mut textures = self.textures.lock();

        if textures.len() >= MAX_TEXTURES {
            anyhow::bail!("Exceeded maximum number of textures");
        }

        let rgba32 = image.to_rgba32f();
        let width = rgba32.width();
        let height = rgba32.height();

        let mut rgba16: Vec<half::f16> = Vec::with_capacity((width * height * 4) as usize);
        for p in rgba32.pixels() {
            rgba16.push(half::f16::from_f32(p.0[0]));
            rgba16.push(half::f16::from_f32(p.0[1]));
            rgba16.push(half::f16::from_f32(p.0[2]));
            rgba16.push(half::f16::from_f32(p.0[3]));
        }

        let texture = self.upload_rgba16f_texture(&rgba16, width, height)?;

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

        let vertex_count = mesh.vertices.len() as u32;
        let index_count = mesh.indices.len() as u32;
        let triangle_count = index_count / 3;
        let vertex_stride = std::mem::size_of::<Vertex>() as u64;

        let vb_base = self.vertex_buffer.lock().buffer.get_device_address();
        let ib_base = self.index_buffer.lock().buffer.get_device_address();

        let vb_addr = vb_base + (vertex_offset.max(0) as u64) * vertex_stride;
        let ib_addr = ib_base + (index_offset as u64) * std::mem::size_of::<u32>() as u64;

        let blas = build_blas(
            &self.engine,
            vb_addr,
            vertex_count,
            vertex_stride,
            ib_addr,
            triangle_count,
        )
        .context("build_blas in add_mesh")?;

        let uploaded = UploadedMesh {
            vertex_offset,
            index_offset,
            index_count,
            blas,
        };

        let handle = {
            let mut mb = self.uploaded_meshes.lock();
            mb.push(uploaded);
            (mb.len() - 1) as MeshHandle
        };

        Ok(handle)
    }

    /// Build a TLAS covering all opaque/masked draw items in `draw_items`.
    /// Returns `None` if there are no geometry items.
    pub fn build_tlas_for_items(
        &self,
        draw_items: &[DrawItem],
    ) -> anyhow::Result<Option<AccelerationStructure>> {
        let mb = self.uploaded_meshes.lock();
        let mut instances: Vec<TlasInstance> = Vec::new();

        for (idx, item) in draw_items.iter().enumerate() {
            let mesh = &mb[item.mesh as usize];
            if mesh.index_count == 0 {
                continue;
            }
            instances.push(TlasInstance::new(
                mesh.blas.get_device_address(),
                item.model_matrix,
                idx as u32,
                0xFF,
            ));
        }

        if instances.is_empty() {
            return Ok(None);
        }

        let tlas = build_tlas(&self.engine, &instances).context("build_tlas_for_items")?;
        Ok(Some(tlas))
    }

    pub fn add_material(&self, material: PbrMaterial) -> anyhow::Result<MaterialHandle> {
        let handle = {
            let mut mb = self.material_buffer.lock();
            mb.add(material)?
        };

        Ok(handle)
    }

    pub fn get_mesh_index_count(&self, handle: MeshHandle) -> u32 {
        self.uploaded_meshes.lock()[handle as usize].index_count
    }

    pub fn get_mesh_vertex_offset(&self, handle: MeshHandle) -> i32 {
        self.uploaded_meshes.lock()[handle as usize].vertex_offset
    }

    pub fn get_mesh_index_offset(&self, handle: MeshHandle) -> u32 {
        self.uploaded_meshes.lock()[handle as usize].index_offset
    }

    /// Returns packed mesh offsets for DrawItem.mesh_offsets:
    ///   low  32 bits = vertex_offset (first vertex in the global VB for this mesh, as u32)
    ///   high 32 bits = index_offset  (first index in the global IB for this mesh)
    /// The RT hit shader reads .x as vertex_offset and .y as index_offset via uint2 mesh_offsets.
    pub fn pack_mesh_offsets(&self, handle: MeshHandle) -> u64 {
        let mb = self.uploaded_meshes.lock();
        let mesh = &mb[handle as usize];
        let vertex_offset = mesh.vertex_offset.max(0) as u32;
        let index_offset = mesh.index_offset;
        (vertex_offset as u64) | ((index_offset as u64) << 32)
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

    /// Rebuild the TLAS for `frame_slot` from the current draw_items and update
    /// the per-slot TLAS descriptor in the meta descriptor set.
    /// Called every frame before the RT pass executes.
    pub fn update_tlas(&self, frame_slot: usize, draw_items: &[DrawItem]) -> anyhow::Result<()> {
        let uploaded = self.uploaded_meshes.lock();
        let instances: Vec<TlasInstance> = draw_items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                let mesh = uploaded.get(item.mesh as usize)?;
                Some(TlasInstance::new(
                    mesh.blas.get_device_address(),
                    item.model_matrix,
                    idx as u32,
                    0xFF,
                ))
            })
            .collect();

        if instances.is_empty() {
            return Ok(());
        }

        let new_tlas = build_tlas(&self.engine, &instances).context("update_tlas: build_tlas")?;

        self.meta_descriptor_set
            .write_acceleration_structure_for_slot(
                frame_slot,
                META_BUFFER_BINDING_TLAS,
                new_tlas.get_raw_handle(),
            );

        *self.tlas[frame_slot].lock() = Some(new_tlas);
        Ok(())
    }

    pub fn get_index_buffer(&self) -> MappedMutexGuard<'_, Buffer> {
        let ib = self.index_buffer.lock();
        MutexGuard::map(ib, |ib: &mut GpuBuffer<u32>| &mut ib.buffer)
    }

    /// Returns the device address of the shared vertex buffer for AS building.
    pub fn get_vertex_buffer_device_address(&self) -> u64 {
        self.vertex_buffer.lock().buffer.get_device_address()
    }

    /// Returns the device address of the shared index buffer for AS building.
    pub fn get_index_buffer_device_address(&self) -> u64 {
        self.index_buffer.lock().buffer.get_device_address()
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
                    | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    | extra_usage,
                memory_property: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                update_frequency,
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
        let staging_size = std::mem::size_of_val(data) as u64;

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
