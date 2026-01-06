use crate::RenderEngine;
use ash::vk;
use std::sync::Arc;
use vk_mem::Allocation;

pub trait Image {
    fn get_image(&self) -> vk::Image;
    fn get_image_view(&self, frame_idx: usize) -> vk::ImageView;
}

pub trait Buffer {}

pub enum UpdateFrequency {
    Static,
    PerFrame,
}

pub struct ImageCreateInfo {
    pub image_type: vk::ImageType,
    pub update_frequency: UpdateFrequency,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub usage: vk::ImageUsageFlags,
}

pub struct MultiImage {
    image: vk::Image,
    image_views: Vec<vk::ImageView>,
    allocation: Allocation,

    engine: Arc<RenderEngine>,
}

// impl MultiImage {
//     pub unsafe fn new(
//         engine: Arc<RenderEngine>,
//         create_info: ImageCreateInfo,
//     ) -> Result<Arc<MultiImage>> {
//         let frames_count  = match create_info.update_frequency {
//             UpdateFrequency::Static => 1,
//             UpdateFrequency::PerFrame => engine.frames.len() as u32,
//         };
// 
//         let image_info = vk::ImageCreateInfo::default()
//             .image_type(create_info.image_type)
//             .format(create_info.format)
//             .extent(vk::Extent3D {
//                 width: create_info.extent.width * frames_count as u32,
//                 height: create_info.extent.height,
//                 depth: 1,
//             })
//             .mip_levels(1)
//             .array_layers(1)
//             .samples(vk::SampleCountFlags::TYPE_1)
//             .tiling(vk::ImageTiling::OPTIMAL)
//             .usage(create_info.usage)
//             .sharing_mode(vk::SharingMode::EXCLUSIVE)
//             .initial_layout(vk::ImageLayout::UNDEFINED);
//     }
// }

impl Image for MultiImage {
    fn get_image(&self) -> vk::Image {
        self.image
    }

    fn get_image_view(&self, frame_idx: usize) -> vk::ImageView {
        if self.image_views.len() > 1 {
            self.image_views[frame_idx]
        } else {
            self.image_views[0]
        }
    }
}
