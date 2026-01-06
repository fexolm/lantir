use std::sync::Arc;
use crate::resource::{DeferDrop, Resource};
use crate::RenderEngine;
use ash::vk;

pub type Shader = Resource<ShaderData>;

impl Shader {
    pub fn new(engine: Arc<RenderEngine>, code: &[u8]) -> anyhow::Result<Arc<Self>> {
        let data = ShaderData::new(&engine, code)?;
        Ok(Arc::new(Resource::make(engine, data)))
    }
}

pub struct ShaderData {
    pub(crate) shader: vk::ShaderModule,
}

impl ShaderData {
    pub fn new(engine: &RenderEngine, code: &[u8]) -> anyhow::Result<Self> {
        let code_aligned = {
            let (prefix, aligned, suffix) = unsafe { code.align_to::<u32>() };
            if !prefix.is_empty() || !suffix.is_empty() {
                return Err(anyhow::anyhow!("Shader code is not properly aligned"));
            }
            aligned
        };

        let create_info = vk::ShaderModuleCreateInfo::default().code(code_aligned);

        let shader = unsafe { engine.device.create_shader_module(&create_info, None)? };

        Ok(Self { shader })
    }
}

impl DeferDrop for ShaderData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            engine.device.destroy_shader_module(self.shader, None);
        }
    }
}
