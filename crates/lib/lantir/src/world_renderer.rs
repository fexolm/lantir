use std::sync::Arc;

use lantir_hal::{
    AccessType, CommandBuffer, CopyImageInfo, ImageBarrier, RenderEngine, Texture,
    TextureCreateInfo, UpdateFrequency, vk,
};

use crate::{
    render_pass::{RenderPass, opaque::OpaquePass},
    resources::resource_manager::ResourceManager,
    scene::Scene,
};

pub struct WorldRendererConfig {
    pub draw_extent: vk::Extent2D,
    pub window_extent: vk::Extent2D,
}

pub struct WorldRenderer {
    engine: Arc<RenderEngine>,
    resource_manager: Arc<ResourceManager>,

    color_target: Arc<Texture>,
    depth_target: Arc<Texture>,
    color_format: vk::Format,
    draw_extent: vk::Extent2D,
    window_extent: vk::Extent2D,

    opaque_pass: OpaquePass,
}

impl WorldRenderer {
    pub fn new(engine: Arc<RenderEngine>, config: WorldRendererConfig) -> anyhow::Result<Self> {
        let resource_manager = Arc::new(ResourceManager::new(engine.clone())?);

        let draw_extent = config.draw_extent;
        let window_extent = config.window_extent;

        let color_format = vk::Format::B8G8R8A8_UNORM;

        let color_target = Arc::new(Texture::new(
            engine.clone(),
            &TextureCreateInfo {
                image_type: vk::ImageType::TYPE_2D,
                update_frequency: UpdateFrequency::PerFrame,
                format: color_format,
                extent: vk::Extent3D {
                    width: draw_extent.width,
                    height: draw_extent.height,
                    depth: 1,
                },
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::SAMPLED,
                aspect: vk::ImageAspectFlags::COLOR,
                mip_levels: 1,
            },
        )?);

        let depth_target = Arc::new(Texture::new(
            engine.clone(),
            &TextureCreateInfo {
                image_type: vk::ImageType::TYPE_2D,
                update_frequency: UpdateFrequency::PerFrame,
                format: vk::Format::D32_SFLOAT,
                extent: vk::Extent3D {
                    width: draw_extent.width,
                    height: draw_extent.height,
                    depth: 1,
                },
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
                aspect: vk::ImageAspectFlags::DEPTH,
                mip_levels: 1,
            },
        )?);

        let opaque_pass = OpaquePass::new(
            &engine,
            &resource_manager,
            color_format,
            color_target.clone(),
            depth_target.clone(),
        )?;

        Ok(Self {
            engine,
            resource_manager,
            color_target,
            depth_target,
            color_format,
            draw_extent,
            window_extent,
            opaque_pass,
        })
    }

    pub fn draw_extent(&self) -> vk::Extent2D {
        self.draw_extent
    }

    pub fn color_target(&self) -> &Texture {
        &*self.color_target
    }

    pub fn get_engine(&self) -> &Arc<RenderEngine> {
        &self.engine
    }

    pub fn depth_target(&self) -> &Texture {
        &*self.depth_target
    }

    pub fn color_format(&self) -> vk::Format {
        self.color_format
    }

    pub fn get_resource_manager(&self) -> &ResourceManager {
        self.resource_manager.as_ref()
    }

    pub fn resource_manager(&self) -> &Arc<ResourceManager> {
        &self.resource_manager
    }

    pub fn resize(&mut self, new_extent: vk::Extent2D) -> anyhow::Result<()> {
        self.engine.recreate_swapchain()?;
        self.window_extent = new_extent;
        Ok(())
    }

    fn run_passes(&self, cb: &CommandBuffer, scene: &Scene) -> anyhow::Result<()> {
        self.opaque_pass.prepare(&self, scene)?;
        self.opaque_pass.execute(&self, scene, cb)?;
        Ok(())
    }

    pub fn draw_frame(&mut self, scene: &Scene) -> anyhow::Result<()> {
        let frame = self.engine.begin_frame()?;

        let swapchain_image = self.engine.acquire_swapchain_image(&frame)?;

        let cb = frame.get_render_command_buffer();
        cb.begin(&self.engine)?;

        cb.cmd_set_viewport(&self.engine, self.draw_extent);
        cb.cmd_set_scissor(&self.engine, self.draw_extent);

        cb.cmd_image_barrier(
            &self.engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::Nothing],
                next_accesses: &[AccessType::ColorAttachmentWrite],
                previous_layout: vk::ImageLayout::UNDEFINED,
                next_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                image: &*self.color_target,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );

        cb.cmd_image_barrier(
            &self.engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::Nothing],
                next_accesses: &[AccessType::DepthStencilAttachmentWrite],
                previous_layout: vk::ImageLayout::UNDEFINED,
                next_layout: vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                image: &*self.depth_target,
                aspect_mask: vk::ImageAspectFlags::DEPTH,
            },
        );

        self.resource_manager.set_draw_items(scene.draw_items)?;
        self.resource_manager.reset_global_indirect_buffer()?;

        cb.cmd_bind_index_buffer(
            &self.engine,
            &*self.resource_manager.get_index_buffer(),
            vk::IndexType::UINT32,
        );

        self.run_passes(cb, scene)?;

        cb.cmd_image_barrier(
            &self.engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::ColorAttachmentWrite],
                next_accesses: &[AccessType::TransferRead],
                previous_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                next_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image: &*self.color_target,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );

        cb.cmd_image_barrier(
            &self.engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::Nothing],
                next_accesses: &[AccessType::TransferWrite],
                previous_layout: vk::ImageLayout::UNDEFINED,
                next_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                image: &swapchain_image,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );

        {
            let copy_extent: vk::Extent2D = vk::Extent2D {
                width: self.window_extent.width.min(self.draw_extent.width),
                height: self.window_extent.height.min(self.draw_extent.height),
            };

            let copy_image_info = CopyImageInfo {
                src_image: &*self.color_target,
                src_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                src_aspect_mask: vk::ImageAspectFlags::COLOR,
                src_extent: copy_extent,
                dst_image: &swapchain_image,
                dst_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                dst_aspect_mask: vk::ImageAspectFlags::COLOR,
                dst_extent: copy_extent,
            };

            cb.cmd_copy_image(&self.engine, &copy_image_info);
        }

        cb.cmd_image_barrier(
            &self.engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::TransferWrite],
                next_accesses: &[AccessType::Present],
                previous_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                next_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                image: &swapchain_image,
                aspect_mask: vk::ImageAspectFlags::COLOR,
            },
        );

        cb.end(&self.engine)?;

        self.engine
            .submit_and_present(frame, &swapchain_image)
            .unwrap();

        Ok(())
    }
}
