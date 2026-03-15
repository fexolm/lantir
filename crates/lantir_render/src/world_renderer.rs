use std::sync::Arc;

use anyhow::Context;
use bevy_ecs::resource::Resource;
use lantir_hal::{
    AccessType, AllocationCreateFlags, Buffer, BufferCreateInfo, CommandBuffer, CopyImageInfo,
    ImageBarrier, RenderEngine, Texture, TextureCreateInfo, UpdateFrequency, vk,
};

use crate::{
    render_pass::{RenderPass, geometry::GeometryPass, rt::RtPass, sky::SkyPass},
    resources::{PbrBlendMode, resource_manager::ResourceManager},
    scene::Scene,
};

pub struct WorldRendererConfig {
    pub draw_extent: vk::Extent2D,
    pub window_extent: vk::Extent2D,
}

#[derive(Resource)]
pub struct WorldRenderer {
    engine: Arc<RenderEngine>,
    resource_manager: Arc<ResourceManager>,

    color_target: Arc<Texture>,
    depth_target: Arc<Texture>,

    /// GBuffer: world-space normals (RGBA16F)
    gbuf_normal: Arc<Texture>,
    /// GBuffer: albedo (RGBA8)
    gbuf_albedo: Arc<Texture>,
    /// GBuffer: roughness (R) + metallic (G) (RG8)
    gbuf_roughness_metal: Arc<Texture>,
    /// GBuffer: motion vectors (RG16F)
    gbuf_motion: Arc<Texture>,

    color_format: vk::Format,
    draw_extent: vk::Extent2D,
    window_extent: vk::Extent2D,

    /// View-projection matrix from the previous frame, used for motion vector generation.
    prev_viewproj: glam::Mat4,

    sky_pass: SkyPass,
    geometry_opaque_pass: GeometryPass,
    geometry_masked_pass: GeometryPass,
    rt_pass: RtPass,
}

impl WorldRenderer {
    pub fn new(engine: Arc<RenderEngine>, config: &WorldRendererConfig) -> anyhow::Result<Self> {
        let resource_manager = Arc::new(ResourceManager::new(engine.clone())?);

        let draw_extent = config.draw_extent;
        let window_extent = config.window_extent;
        let color_format = vk::Format::B8G8R8A8_UNORM;

        // Main color target (skybox + RT composite output)
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
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::STORAGE,
                aspect: vk::ImageAspectFlags::COLOR,
                mip_levels: 1,
            },
        )?);

        // Depth target (shared between sky and geometry passes)
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
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                aspect: vk::ImageAspectFlags::DEPTH,
                mip_levels: 1,
            },
        )?);

        // GBuffer: normal (RGBA16F) — written by geometry pass, read by RT raygen
        let gbuf_normal = Arc::new(Texture::new(
            engine.clone(),
            &TextureCreateInfo {
                image_type: vk::ImageType::TYPE_2D,
                update_frequency: UpdateFrequency::PerFrame,
                format: vk::Format::R16G16B16A16_SFLOAT,
                extent: vk::Extent3D {
                    width: draw_extent.width,
                    height: draw_extent.height,
                    depth: 1,
                },
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                aspect: vk::ImageAspectFlags::COLOR,
                mip_levels: 1,
            },
        )?);

        // GBuffer: albedo (RGBA8)
        let gbuf_albedo = Arc::new(Texture::new(
            engine.clone(),
            &TextureCreateInfo {
                image_type: vk::ImageType::TYPE_2D,
                update_frequency: UpdateFrequency::PerFrame,
                format: vk::Format::R8G8B8A8_UNORM,
                extent: vk::Extent3D {
                    width: draw_extent.width,
                    height: draw_extent.height,
                    depth: 1,
                },
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                aspect: vk::ImageAspectFlags::COLOR,
                mip_levels: 1,
            },
        )?);

        // GBuffer: roughness/metallic (RG8)
        let gbuf_roughness_metal = Arc::new(Texture::new(
            engine.clone(),
            &TextureCreateInfo {
                image_type: vk::ImageType::TYPE_2D,
                update_frequency: UpdateFrequency::PerFrame,
                format: vk::Format::R8G8_UNORM,
                extent: vk::Extent3D {
                    width: draw_extent.width,
                    height: draw_extent.height,
                    depth: 1,
                },
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                aspect: vk::ImageAspectFlags::COLOR,
                mip_levels: 1,
            },
        )?);

        // GBuffer: motion vectors (RG16F) — written by geometry pass, read by RT raygen
        let gbuf_motion = Arc::new(Texture::new(
            engine.clone(),
            &TextureCreateInfo {
                image_type: vk::ImageType::TYPE_2D,
                update_frequency: UpdateFrequency::PerFrame,
                format: vk::Format::R16G16_SFLOAT,
                extent: vk::Extent3D {
                    width: draw_extent.width,
                    height: draw_extent.height,
                    depth: 1,
                },
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                aspect: vk::ImageAspectFlags::COLOR,
                mip_levels: 1,
            },
        )?);

        let sky_pass = SkyPass::new(
            &engine,
            &resource_manager,
            color_format,
            color_target.clone(),
            depth_target.clone(),
        )?;

        let geometry_opaque_pass = GeometryPass::new(
            &engine,
            &resource_manager,
            gbuf_normal.clone(),
            gbuf_albedo.clone(),
            gbuf_roughness_metal.clone(),
            gbuf_motion.clone(),
            depth_target.clone(),
            PbrBlendMode::Opaque,
        )?;

        let geometry_masked_pass = GeometryPass::new(
            &engine,
            &resource_manager,
            gbuf_normal.clone(),
            gbuf_albedo.clone(),
            gbuf_roughness_metal.clone(),
            gbuf_motion.clone(),
            depth_target.clone(),
            PbrBlendMode::Masked,
        )?;

        let rt_pass = RtPass::new(
            &engine,
            &resource_manager,
            draw_extent,
            color_target.clone(),
            gbuf_normal.clone(),
            gbuf_albedo.clone(),
            gbuf_roughness_metal.clone(),
            gbuf_motion.clone(),
            depth_target.clone(),
        )?;

        Ok(Self {
            engine,
            resource_manager,
            color_target,
            depth_target,
            gbuf_normal,
            gbuf_albedo,
            gbuf_roughness_metal,
            gbuf_motion,
            color_format,
            draw_extent,
            window_extent,
            prev_viewproj: glam::Mat4::IDENTITY,
            sky_pass,
            geometry_opaque_pass,
            geometry_masked_pass,
            rt_pass,
        })
    }

    pub fn draw_extent(&self) -> vk::Extent2D {
        self.draw_extent
    }

    pub fn color_target(&self) -> &Texture {
        &self.color_target
    }

    pub fn get_engine(&self) -> &Arc<RenderEngine> {
        &self.engine
    }

    pub fn depth_target(&self) -> &Texture {
        &self.depth_target
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

    /// Returns the view-projection matrix from the previous frame.
    /// Used by the geometry pass to generate motion vectors.
    pub fn prev_viewproj(&self) -> glam::Mat4 {
        self.prev_viewproj
    }

    pub fn resize(&mut self, new_extent: vk::Extent2D) -> anyhow::Result<()> {
        self.engine.recreate_swapchain(new_extent)?;
        self.window_extent = new_extent;
        Ok(())
    }

    fn run_passes(&self, cb: &CommandBuffer, scene: &Scene) -> anyhow::Result<()> {
        // SkyPass: clears color_target and depth_target, renders skybox into color_target.
        // The depth buffer is populated here (reverse-Z, sky writes no depth so depth stays 0).
        self.sky_pass.execute(self, scene, cb)?;

        // Geometry passes: write GBuffer from depth-tested geometry.
        // The depth from SkyPass is reused (LOAD).
        // GBuffer attachments are cleared by the opaque pass (CLEAR load op on first use).
        self.geometry_opaque_pass.execute(self, scene, cb)?;
        self.geometry_masked_pass.execute(self, scene, cb)?;

        // After geometry passes, transition GBuffer color attachments to SHADER_READ_ONLY_OPTIMAL
        // so the RT pass can read them as sampled images. The descriptor was written with
        // SHADER_READ_ONLY_OPTIMAL layout, so the actual image layout must match at trace time.
        for tex in [
            &*self.gbuf_normal,
            &*self.gbuf_albedo,
            &*self.gbuf_roughness_metal,
            &*self.gbuf_motion,
        ] {
            cb.cmd_image_barrier(
                &self.engine,
                &ImageBarrier {
                    previous_accesses: &[AccessType::ColorAttachmentWrite],
                    next_accesses: &[
                        AccessType::RayTracingShaderReadSampledImageOrUniformTexelBuffer,
                    ],
                    previous_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    next_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    image: tex,
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                },
            );
        }

        // Transition depth to DEPTH_READ_ONLY_OPTIMAL for the RT pass sampled read.
        cb.cmd_image_barrier(
            &self.engine,
            &ImageBarrier {
                previous_accesses: &[AccessType::DepthStencilAttachmentWrite],
                next_accesses: &[AccessType::RayTracingShaderReadSampledImageOrUniformTexelBuffer],
                previous_layout: vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                next_layout: vk::ImageLayout::DEPTH_READ_ONLY_OPTIMAL,
                image: &*self.depth_target,
                aspect_mask: vk::ImageAspectFlags::DEPTH,
            },
        );

        // RT pass: reads GBuffer (now in SHADER_READ_ONLY_OPTIMAL / DEPTH_READ_ONLY_OPTIMAL),
        // traces shadow rays, writes final lit image into color_target.
        self.rt_pass.execute(self, scene, cb)?;

        Ok(())
    }

    /// Reads back the last rendered frame and saves it as a PNG.
    /// Must be called after `draw_frame()`. Blocks until the GPU is idle.
    pub fn dump_frame_to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        use anyhow::Context as _;

        let width = self.draw_extent.width;
        let height = self.draw_extent.height;
        let size = (width * height * 4) as vk::DeviceSize;

        self.engine
            .wait_idle()
            .context("wait_idle before readback")?;

        let readback = Buffer::new(
            self.engine.clone(),
            &BufferCreateInfo {
                size,
                usage: vk::BufferUsageFlags::TRANSFER_DST,
                update_frequency: UpdateFrequency::Static,
                memory_property: vk::MemoryPropertyFlags::HOST_VISIBLE
                    | vk::MemoryPropertyFlags::HOST_COHERENT,
                vma_flags: AllocationCreateFlags::MAPPED
                    | AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
            },
        )
        .context("create readback buffer")?;

        self.engine
            .immediate_submit(|cb| {
                cb.cmd_copy_image_to_buffer(
                    &self.engine,
                    &*self.color_target,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    &readback,
                    width,
                    height,
                );
            })
            .context("copy image to buffer")?;

        let pixels = unsafe {
            let ptr = readback.map().context("map readback buffer")?;
            let slice = std::slice::from_raw_parts(ptr as *const u8, size as usize);
            let owned = slice.to_vec();
            readback.unmap();
            owned
        };

        // VK_FORMAT_B8G8R8A8_UNORM: bytes are [B, G, R, A]. PNG expects [R, G, B, A].
        let mut rgba = vec![0u8; size as usize];
        for i in 0..(width * height) as usize {
            rgba[i * 4] = pixels[i * 4 + 2];
            rgba[i * 4 + 1] = pixels[i * 4 + 1];
            rgba[i * 4 + 2] = pixels[i * 4];
            rgba[i * 4 + 3] = pixels[i * 4 + 3];
        }

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create dirs for {}", path.display()))?;
            }
        }

        image::save_buffer(path, &rgba, width, height, image::ColorType::Rgba8)
            .with_context(|| format!("save PNG to {}", path.display()))?;

        Ok(())
    }

    pub fn draw_frame(&mut self, scene: &Scene) -> anyhow::Result<()> {
        let frame = self.engine.begin_frame().context("engine.begin_frame")?;

        let swapchain_image = self
            .engine
            .acquire_swapchain_image(frame)
            .context("engine.acquire_swapchain_image")?;

        let cb = frame.get_render_command_buffer();
        cb.begin(&self.engine).context("command_buffer.begin")?;

        cb.cmd_set_viewport(&self.engine, self.draw_extent);
        cb.cmd_set_scissor(&self.engine, self.draw_extent);

        // Transition color_target to COLOR_ATTACHMENT_OPTIMAL for the sky pass.
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
        // Transition depth to DEPTH_ATTACHMENT_OPTIMAL for the sky + geometry passes.
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

        // Transition GBuffer attachments to COLOR_ATTACHMENT_OPTIMAL for the geometry pass.
        for tex in [
            &*self.gbuf_normal,
            &*self.gbuf_albedo,
            &*self.gbuf_roughness_metal,
            &*self.gbuf_motion,
        ] {
            cb.cmd_image_barrier(
                &self.engine,
                &ImageBarrier {
                    previous_accesses: &[AccessType::Nothing],
                    next_accesses: &[AccessType::ColorAttachmentWrite],
                    previous_layout: vk::ImageLayout::UNDEFINED,
                    next_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    image: tex,
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                },
            );
        }

        self.resource_manager
            .set_draw_items(scene.draw_items)
            .context("resource_manager.set_draw_items")?;
        self.resource_manager
            .reset_global_indirect_buffer()
            .context("resource_manager.reset_global_indirect_buffer")?;

        cb.cmd_bind_index_buffer(
            &self.engine,
            &self.resource_manager.get_index_buffer(),
            vk::IndexType::UINT32,
        );

        let frame_slot = self.engine.get_current_frame_index();
        self.resource_manager
            .update_tlas(frame_slot, scene.draw_items)
            .context("resource_manager.update_tlas")?;

        self.run_passes(cb, scene).context("run_passes")?;

        // After run_passes, color_target is in COLOR_ATTACHMENT_OPTIMAL
        // (left in that state after RtPass copies rt_output into it).
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

        let copy_extent = vk::Extent2D {
            width: self.window_extent.width.min(self.draw_extent.width),
            height: self.window_extent.height.min(self.draw_extent.height),
        };

        cb.cmd_copy_image(
            &self.engine,
            &CopyImageInfo {
                src_image: &*self.color_target,
                src_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                src_aspect_mask: vk::ImageAspectFlags::COLOR,
                src_extent: copy_extent,
                dst_image: &swapchain_image,
                dst_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                dst_aspect_mask: vk::ImageAspectFlags::COLOR,
                dst_extent: copy_extent,
            },
        );

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

        cb.end(&self.engine).context("command_buffer.end")?;

        self.engine
            .submit_and_present(frame, &swapchain_image)
            .unwrap();

        // Store this frame's viewproj as prev_viewproj for the next frame's motion vectors.
        self.prev_viewproj = scene.camera.viewproj;

        Ok(())
    }
}
