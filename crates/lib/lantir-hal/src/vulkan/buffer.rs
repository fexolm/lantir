use ash::vk;

pub trait VulkanImage {
    fn get_image(&self) -> vk::Image;
    fn get_image_view(&self) -> vk::ImageView;
}

pub trait VulkanBuffer {}
