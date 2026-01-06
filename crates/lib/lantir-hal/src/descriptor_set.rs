use crate::resource::{DeferDrop, Resource};
use crate::{Image, RenderEngine};
use ash::vk;
use std::sync::Arc;

pub struct DescriptorSetBinding {
    pub typ: vk::DescriptorType,
    pub binding: u32,
    pub stage: vk::ShaderStageFlags,
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
}

impl DescriptorSet {
    pub fn new(
        engine: Arc<RenderEngine>,
        layout: Arc<DescriptorSetLayout>,
    ) -> anyhow::Result<Self> {
        let data = DescriptorSetData::new(&engine, layout)?;
        Ok(Resource::make(engine, data))
    }

    pub(crate) fn get(&self) -> vk::DescriptorSet {
        self.descriptor_sets[self.engine.get_current_frame_index()]
    }

    pub fn write_image(&self, image_info: &WriteImageInfo) {
        let img_infos = [vk::DescriptorImageInfo::default()
            .image_view(image_info.image.get_image_view())
            .image_layout(image_info.layout)];

        let descriptor_write = vk::WriteDescriptorSet::default()
            .dst_set(self.get())
            .dst_binding(image_info.binding)
            .descriptor_type(image_info.descriptor_type)
            .image_info(&img_infos);

        unsafe {
            self.engine
                .device
                .update_descriptor_sets(&[descriptor_write], &[]);
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
                descriptor_count: 1,
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

    layout: Arc<DescriptorSetLayout>,
}

impl DescriptorSetData {
    pub fn new(engine: &RenderEngine, layout: Arc<DescriptorSetLayout>) -> anyhow::Result<Self> {
        let layouts = (0..engine.frames.len())
            .map(|_| layout.layout)
            .collect::<Vec<_>>();

        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(engine.descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe { engine.device.allocate_descriptor_sets(&alloc_info)? };

        Ok(DescriptorSetData {
            descriptor_sets,
            layout,
        })
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
