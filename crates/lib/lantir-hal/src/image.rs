use crate::resource::{Resource, ResourceDrop};
use crate::RenderEngine;
use ash::vk;
use vk_mem::{Alloc, Allocation, AllocationCreateInfo, MemoryUsage};

pub trait Image {
    fn get_image(&self, frame: usize) -> vk::Image;
    fn get_image_view(&self, frame: usize) -> vk::ImageView;
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

pub type Texture = Resource<TextureData>;

impl Image for Texture {
    fn get_image(&self, frame: usize) -> vk::Image {
        self.get_handle().get_image(frame)
    }

    fn get_image_view(&self, frame: usize) -> vk::ImageView {
        self.get_handle().get_image_view(frame)
    }
}

pub struct TextureData {
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,

    // TODO: use single allocation for all images
    allocations: Vec<Allocation>,
}

impl TextureData {
    pub fn new(
        engine: &RenderEngine,
        create_info: &TextureCreateInfo,
    ) -> anyhow::Result<TextureData> {
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

        Ok(TextureData {
            images,
            image_views,
            allocations,
        })
    }
}

impl ResourceDrop for TextureData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe {
            for &image_view in &self.image_views {
                engine.device.destroy_image_view(image_view, None);
            }
            for (i, &image) in self.images.iter().enumerate() {
                engine
                    .allocator
                    .destroy_image(image, &mut self.allocations[i]);
            }
        }
    }
}

impl Image for TextureData {
    fn get_image(&self, frame: usize) -> vk::Image {
        if self.images.len() > 1 {
            self.images[frame]
        } else {
            self.images[0]
        }
    }

    fn get_image_view(&self, frame: usize) -> vk::ImageView {
        if self.image_views.len() > 1 {
            self.image_views[frame]
        } else {
            self.image_views[0]
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
