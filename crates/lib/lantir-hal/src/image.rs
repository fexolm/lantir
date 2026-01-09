use crate::resource::{DeferDrop, Resource};
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
    pub extent: vk::Extent3D,
    pub usage: vk::ImageUsageFlags,
    pub aspect: vk::ImageAspectFlags,
    pub mip_levels: u32,
}

pub type Texture = Resource<TextureData>;

impl Texture {
    pub fn new(
        engine: Arc<RenderEngine>,
        create_info: &TextureCreateInfo,
    ) -> anyhow::Result<Texture> {
        let data = TextureData::new(&engine, create_info)?;
        Ok(Self::make(engine, data))
    }
}

impl Image for Texture {
    fn get_image(&self) -> vk::Image {
        self.get_handle()
            .get_image(self.engine.get_current_frame_index())
    }

    fn get_image_view(&self) -> vk::ImageView {
        self.get_handle()
            .get_image_view(self.engine.get_current_frame_index())
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
            .extent(create_info.extent)
            .mip_levels(create_info.mip_levels)
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
                    create_info.aspect,
                )?);
            }
        }

        Ok(TextureData {
            images,
            image_views,
            allocations,
        })
    }

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

impl DeferDrop for TextureData {
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

pub type Sampler = Resource<SamplerData>;

impl Sampler {
    pub fn new(engine: Arc<RenderEngine>, info: &SamplerInfo) -> anyhow::Result<Sampler> {
        let data = SamplerData::new(&engine, info)?;
        Ok(Resource::make(engine, data))
    }
}

pub struct SamplerData {
    pub(crate) sampler: vk::Sampler,
}

pub struct SamplerInfo {
    pub filter: vk::Filter,
}

impl SamplerData {
    pub fn new(engine: &RenderEngine, info: &SamplerInfo) -> anyhow::Result<SamplerData> {
        unsafe {
            let create_info = vk::SamplerCreateInfo::default()
                .mag_filter(info.filter)
                .min_filter(info.filter);

            let sampler = engine.device.create_sampler(&create_info, None)?;
            Ok(SamplerData { sampler })
        }
    }
}

impl DeferDrop for SamplerData {
    fn destroy(&mut self, engine: &RenderEngine) {
        unsafe { engine.device.destroy_sampler(self.sampler, None) };
    }
}
