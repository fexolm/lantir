use crate::barriers::{AspectMask, ImageBarrier, ImageLayout};
use crate::vulkan::VulkanCommandBuffer;
use crate::{Image, RenderingEngine};

pub struct CommandBuffer<'i> {
    pub(crate) imp: &'i VulkanCommandBuffer,
}

impl<'i> CommandBuffer<'i> {
    pub fn cmd_image_barrier(&self, engine: &RenderingEngine, barrier: &ImageBarrier) {
        self.imp.cmd_image_barrier(&engine.imp, barrier);
    }

    pub fn cmd_clear_color(
        &self,
        engine: &RenderingEngine,
        image: &'i dyn Image<'i>,
        layout: ImageLayout,
        color: [f32; 4],
        aspect_mask: AspectMask,
    ) {
        self.imp
            .cmd_clear_color(&engine.imp, image.get_imp(), layout, color, aspect_mask);
    }
}
