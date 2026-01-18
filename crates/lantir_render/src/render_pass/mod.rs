pub mod opaque;
pub mod transparent;

use lantir_hal::CommandBuffer;

use crate::{scene::Scene, world_renderer::WorldRenderer};

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
