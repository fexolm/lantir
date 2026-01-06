use crate::RenderEngine;
use ash::vk;
use std::sync::Arc;
use vk_mem::{Alloc, Allocation, AllocationCreateInfo, MemoryUsage};

pub trait Image {
    fn get_image(&self) -> vk::Image;
    fn get_image_view(&self) -> vk::ImageView;
}

pub enum UpdateFrequency {
    Static,
    PerFrame,
}

pub struct TextureCreateInfo {
    pub image_type: vk::ImageType,
    pub update_frequency: UpdateFrequency,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub usage: vk::ImageUsageFlags,
}

#[derive(Clone)]
pub struct Texture {
    imp: Arc<TextureImpl>,
}

impl Texture {
    pub fn new(
        engine: Arc<RenderEngine>,
        create_info: &TextureCreateInfo,
    ) -> anyhow::Result<Texture> {
        let imp = TextureImpl::new(engine, create_info)?;
        Ok(Texture { imp: Arc::new(imp) })
    }
}

impl Image for Texture {
    fn get_image(&self) -> vk::Image {
        if self.imp.images.len() > 1 {
            self.imp.images[self.imp.engine.get_current_frame_index()]
        } else {
            self.imp.images[0]
        }
    }

    fn get_image_view(&self) -> vk::ImageView {
        if self.imp.image_views.len() > 1 {
            self.imp.image_views[self.imp.engine.get_current_frame_index()]
        } else {
            self.imp.image_views[0]
        }
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        self.imp.engine.schedule_resource_release(self.imp.clone());
    }
}

struct TextureImpl {
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,

    // TODO: use single allocation for all images
    allocations: Vec<Allocation>,

    engine: Arc<RenderEngine>,
}

impl TextureImpl {
    pub fn new(
        engine: Arc<RenderEngine>,
        create_info: &TextureCreateInfo,
    ) -> anyhow::Result<TextureImpl> {
        let frames_count = match create_info.update_frequency {
            UpdateFrequency::Static => 1,
            UpdateFrequency::PerFrame => engine.frames.len() as u32,
        };

        let image_info = vk::ImageCreateInfo::default()
            .image_type(create_info.image_type)
            .format(create_info.format)
            .extent(vk::Extent3D {
                width: create_info.extent.width,
                height: create_info.extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(create_info.usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let allocation_info = AllocationCreateInfo {
            usage: MemoryUsage::AutoPreferDevice,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        };

        let mut images = Vec::new();
        let mut image_views = Vec::new();
        let mut allocations = Vec::new();

        for _frame in 0..frames_count {
            unsafe {
                let (image, allocation) = engine
                    .allocator
                    .create_image(&image_info, &allocation_info)
                    .map_err(|e| anyhow::anyhow!("Failed to allocate image: {}", e))?;
                images.push(image);
                allocations.push(allocation);
                image_views.push(create_imageview(
                    &engine.device,
                    image,
                    create_info.format,
                    vk::ImageAspectFlags::COLOR,
                )?);
            }
        }

        Ok(TextureImpl {
            images,
            image_views,
            allocations,
            engine,
        })
    }
}

impl Drop for TextureImpl {
    fn drop(&mut self) {
        unsafe {
            for &image_view in &self.image_views {
                self.engine.device.destroy_image_view(image_view, None);
            }
            for (i, &image) in self.images.iter().enumerate() {
                self.engine
                    .allocator
                    .destroy_image(image, &mut self.allocations[i]);
            }
        }
    }
}

fn create_imageview(
    device: &ash::Device,
    image: vk::Image,
    format: vk::Format,
    aspect_mask: vk::ImageAspectFlags,
) -> anyhow::Result<vk::ImageView> {
    let view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(
            vk::ImageSubresourceRange::default()
                .aspect_mask(aspect_mask)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1),
        );

    let image_view = unsafe { device.create_image_view(&view_info, None)? };
    Ok(image_view)
}
