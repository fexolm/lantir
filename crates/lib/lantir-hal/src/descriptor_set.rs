use crate::RenderEngine;
use ash::vk;
use std::sync::Arc;
use crate::resource::{Resource, ResourceDrop};

pub struct DescriptorSetBinding {
    pub typ: vk::DescriptorType,
    pub binding: u32,
    pub stage: vk::ShaderStageFlags,
}
pub struct DescriptorSetLayoutCreateInfo {
    pub bindings: Vec<DescriptorSetBinding>,
}

struct DescriptorSetLayoutData {
    pub(crate) layout: vk::DescriptorSetLayout,

    engine: Arc<RenderEngine>,
}

pub type DescriptorSetLayout = Resource<DescriptorSetLayoutData>;

impl DescriptorSetLayoutData {
    pub fn new(
        engine: Arc<RenderEngine>,
        create_info: &DescriptorSetLayoutCreateInfo,
    ) -> anyhow::Result<Self> {
        let bindings = create_info
            .bindings
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

        Ok(DescriptorSetLayoutData { layout, engine })
    }
}

impl ResourceDrop for DescriptorSetLayoutData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            engine.device.destroy_descriptor_set_layout(self.layout, None);
        }
    }
}