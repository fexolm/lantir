use crate::RenderEngine;
use ash::vk;
use std::sync::Arc;

pub struct PipelineLayout {
    pub(crate) layout: vk::PipelineLayout,

    engine: Arc<RenderEngine>,
}
