use crate::image::Sampler;
use crate::resource::{DeferDrop, Resource};
use crate::{Buffer, Image, RenderEngine, UpdateFrequency};
use ash::vk;
use std::sync::Arc;

pub struct DescriptorSetBinding {
    pub typ: vk::DescriptorType,
    pub binding: u32,
    pub stage: vk::ShaderStageFlags,
    pub count: u32,
}
pub type DescriptorSetLayout = Resource<DescriptorSetLayoutData>;

impl DescriptorSetLayout {
    pub fn new(
        engine: Arc<RenderEngine>,
        bindings: &[DescriptorSetBinding],
    ) -> anyhow::Result<Arc<Self>> {
        let data = DescriptorSetLayoutData::new(&engine, bindings)?;
        Ok(Arc::new(Resource::make(engine, data)))
    }
}

pub type DescriptorSet = Resource<DescriptorSetData>;

pub struct WriteImageInfo<'i> {
    pub binding: u32,
    pub image: &'i dyn Image,
    pub layout: vk::ImageLayout,
    pub descriptor_type: vk::DescriptorType,
    pub sampler: Option<&'i Sampler>,
    pub array_index: u64,
}

pub struct WriteBufferInfo<'i> {
    pub binding: u32,
    pub buffer: &'i Buffer,
    pub size: u64,
    pub offset: u64,
    pub descriptor_type: vk::DescriptorType,
}

impl DescriptorSet {
    pub fn new(
        engine: Arc<RenderEngine>,
        layout: Arc<DescriptorSetLayout>,
        update_frequency: UpdateFrequency,
    ) -> anyhow::Result<Self> {
        let data = DescriptorSetData::new(&engine, layout, update_frequency)?;
        Ok(Resource::make(engine, data))
    }

    pub(crate) fn get(&self) -> vk::DescriptorSet {
        self.get_handle().get(self.engine.get_current_frame_index())
    }

    pub fn write_image(&self, image_info: &WriteImageInfo) {
        let mut img_info = vk::DescriptorImageInfo::default()
            .image_view(image_info.image.get_image_view())
            .image_layout(image_info.layout);

        if let Some(sampler) = image_info.sampler {
            img_info = img_info.sampler(sampler.sampler);
        }

        let img_infos = [img_info];

        for &dst_set in &self.descriptor_sets {
            let descriptor_write = vk::WriteDescriptorSet::default()
                .dst_set(dst_set)
                .dst_binding(image_info.binding)
                .descriptor_type(image_info.descriptor_type)
                .image_info(&img_infos)
                .dst_array_element(image_info.array_index as u32);

            unsafe {
                self.engine
                    .device
                    .update_descriptor_sets(&[descriptor_write], &[]);
            }
        }
    }

    pub fn write_buffer(&self, buffer_info: &WriteBufferInfo) {
        for (frame_index, &dst_set) in self.descriptor_sets.iter().enumerate() {
            let buffer_infos = [vk::DescriptorBufferInfo::default()
                .buffer(buffer_info.buffer.get_handle().get_buffer(frame_index))
                .offset(buffer_info.offset)
                .range(buffer_info.size)];

            let descriptor_write = vk::WriteDescriptorSet::default()
                .dst_set(dst_set)
                .dst_binding(buffer_info.binding)
                .descriptor_type(buffer_info.descriptor_type)
                .buffer_info(&buffer_infos);

            unsafe {
                self.engine
                    .device
                    .update_descriptor_sets(&[descriptor_write], &[]);
            }
        }
    }
}

pub struct DescriptorSetLayoutData {
    pub(crate) layout: vk::DescriptorSetLayout,
}

impl DescriptorSetLayoutData {
    pub fn new(engine: &RenderEngine, bindings: &[DescriptorSetBinding]) -> anyhow::Result<Self> {
        let bindings = bindings
            .iter()
            .map(|b| vk::DescriptorSetLayoutBinding {
                binding: b.binding,
                descriptor_type: b.typ,
                descriptor_count: b.count,
                stage_flags: b.stage,
                p_immutable_samplers: std::ptr::null(),
                _marker: std::marker::PhantomData,
            })
            .collect::<Vec<_>>();

        let flags = vk::DescriptorSetLayoutCreateFlags::empty();

        let layout_create_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&bindings)
            .flags(flags);

        let layout = unsafe {
            engine
                .device
                .create_descriptor_set_layout(&layout_create_info, None)?
        };

        Ok(DescriptorSetLayoutData { layout })
    }
}

impl DeferDrop for DescriptorSetLayoutData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            engine
                .device
                .destroy_descriptor_set_layout(self.layout, None);
        }
    }
}

pub struct DescriptorSetData {
    descriptor_sets: Vec<vk::DescriptorSet>,
    pub layout: Arc<DescriptorSetLayout>,
}

impl DescriptorSetData {
    pub fn new(
        engine: &RenderEngine,
        layout: Arc<DescriptorSetLayout>,
        update_frequency: UpdateFrequency,
    ) -> anyhow::Result<Self> {
        let frames_count = match update_frequency {
            UpdateFrequency::Static => 1,
            UpdateFrequency::PerFrame => engine.frames.len() as u32,
        };

        let layouts = (0..frames_count).map(|_| layout.layout).collect::<Vec<_>>();

        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(engine.descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe { engine.device.allocate_descriptor_sets(&alloc_info)? };

        Ok(DescriptorSetData {
            descriptor_sets,
            layout,
        })
    }

    fn get(&self, frame: usize) -> vk::DescriptorSet {
        if self.descriptor_sets.len() == 1 {
            self.descriptor_sets[0]
        } else {
            self.descriptor_sets[frame]
        }
    }
}

impl DeferDrop for DescriptorSetData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            engine
                .device
                .free_descriptor_sets(engine.descriptor_pool, &self.descriptor_sets)
                .unwrap();
        }
    }
}
