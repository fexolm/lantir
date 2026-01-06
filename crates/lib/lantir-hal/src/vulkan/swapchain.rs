use crate::vulkan::buffer::VulkanImage;
use crate::vulkan::device::VulkanDevice;
use crate::vulkan::instance::VulkanInstance;
use crate::vulkan::surface::VulkanSurface;
use crate::vulkan::VulkanFrame;
use ash::khr::swapchain;
use ash::vk;
use std::ops::Deref;

pub struct VulkanSwapchain {
    swapchain: vk::SwapchainKHR,
    loader: swapchain::Device,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    render_finished_semaphores: Vec<vk::Semaphore>,
}

pub struct VulkanSwapchainImage {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub index: u32,
    pub render_finished_semaphore: vk::Semaphore,
}
impl VulkanImage for VulkanSwapchainImage {
    fn get_image(&self) -> vk::Image {
        self.image
    }

    fn get_image_view(&self) -> vk::ImageView {
        self.view
    }
}

impl VulkanSwapchain {
    pub unsafe fn new(
        instance: &VulkanInstance,
        device: &VulkanDevice,
        surface: &VulkanSurface,
    ) -> anyhow::Result<Self> {
        let loader = swapchain::Device::new(&instance, &device);

        let surface_caps = surface
            .get_physical_device_surface_capabilities(device.physical_device, surface.get_raw())?;

        let swapchain = {
            let create_info = vk::SwapchainCreateInfoKHR::default()
                .surface(surface.get_raw())
                .min_image_count(3)
                .image_color_space(vk::ColorSpaceKHR::SRGB_NONLINEAR)
                .image_format(vk::Format::B8G8R8A8_UNORM)
                .image_extent(surface_caps.current_extent)
                .image_usage(
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST,
                )
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(vk::PresentModeKHR::FIFO)
                .pre_transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
                .clipped(true)
                .image_array_layers(1);

            loader.create_swapchain(&create_info, None)?
        };

        let images = loader.get_swapchain_images(swapchain)?;

        let views = images
            .iter()
            .map(|image| {
                create_image_view(
                    device,
                    *image,
                    1,
                    vk::Format::B8G8R8A8_UNORM,
                    vk::ImageAspectFlags::COLOR,
                )
            })
            .collect::<Result<_, _>>()?;

        let render_finished_semaphores = (0..images.len())
            .map(|_| device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None))
            .collect::<Result<_, _>>()?;

        Ok(VulkanSwapchain {
            swapchain,
            loader,
            images,
            views,
            render_finished_semaphores,
        })
    }

    pub fn get_raw(&self) -> vk::SwapchainKHR {
        self.swapchain
    }

    pub unsafe fn acquire_next_image(
        &self,
        frame: &VulkanFrame,
    ) -> anyhow::Result<VulkanSwapchainImage> {
        let acquire_semaphore = frame.swapchain_acquire_semaphore;

        let (index, _) = self.loader.acquire_next_image(
            self.get_raw(),
            u64::MAX,
            acquire_semaphore,
            vk::Fence::null(),
        )?;

        Ok(VulkanSwapchainImage {
            image: self.images[index as usize],
            view: self.views[index as usize],
            render_finished_semaphore: self.render_finished_semaphores[index as usize],
            index,
        })
    }

    pub fn present(
        &self,
        device: &VulkanDevice,
        image: &VulkanSwapchainImage,
    ) -> anyhow::Result<()> {
        let swapchains = [self.get_raw()];
        let wait_semaphores = [image.render_finished_semaphore];
        let image_indices = [image.index];

        let present_info = vk::PresentInfoKHR::default()
            .swapchains(&swapchains)
            .wait_semaphores(&wait_semaphores)
            .image_indices(&image_indices);

        unsafe {
            self.queue_present(device.universal_queue, &present_info)?;
        }

        Ok(())
    }

    pub unsafe fn destroy(&self, device: &VulkanDevice) {
        for &v in &self.views {
            device.destroy_image_view(v, None);
        }

        for &s in &self.render_finished_semaphores {
            device.destroy_semaphore(s, None);
        }

        self.loader.destroy_swapchain(self.swapchain, None);
    }
}

impl Deref for VulkanSwapchain {
    type Target = swapchain::Device;

    fn deref(&self) -> &Self::Target {
        &self.loader
    }
}

fn create_image_view(
    device: &ash::Device,
    image: vk::Image,
    mip_levels: u32,
    format: vk::Format,
    aspect_mask: vk::ImageAspectFlags,
) -> anyhow::Result<vk::ImageView> {
    let create_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask,
            base_mip_level: 0,
            level_count: mip_levels,
            base_array_layer: 0,
            layer_count: 1,
        });

    unsafe { Ok(device.create_image_view(&create_info, None)?) }
}
