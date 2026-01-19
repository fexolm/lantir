pub mod opaque;
pub mod sky;
pub mod transparent;

use lantir_hal::CommandBuffer;

use crate::{scene::Scene, world_renderer::WorldRenderer};

#[repr(C)]
#[derive(Copy, Clone)]
pub struct DynamicConstants {
    pub viewproj: glam::Mat4,
    pub inv_viewproj: glam::Mat4,
    pub camera_pos: glam::Vec4,
}

impl Default for DynamicConstants {
    fn default() -> Self {
        Self {
            viewproj: glam::Mat4::IDENTITY,
            inv_viewproj: glam::Mat4::IDENTITY,
            camera_pos: glam::Vec4::ZERO,
        }
    }
}

pub trait RenderPass {
    fn name(&self) -> &'static str;

    fn prepare(&self, _renderer: &WorldRenderer, _scene: &Scene) -> anyhow::Result<()> {
        Ok(())
    }

    fn execute(
        &self,
        renderer: &WorldRenderer,
        scene: &Scene,
        cb: &CommandBuffer,
    ) -> anyhow::Result<()>;
}
