// use crate::RenderEngine;
// use ash::vk;
// use std::sync::Arc;
// 
// pub struct DescriptorSetLayout {
//     resource: Arc<DescriptorSetLayoutResource>,
// }
// 
// impl Drop for DescriptorSetLayout {
//     fn drop(&mut self) {
//         self.resource
//             .engine
//             .schedule_resource_release(self.resource.clone());
//     }
// }
// 
// struct DescriptorSetLayoutResource {
//     pub(crate) layout: vk::DescriptorSetLayout,
// 
//     engine: Arc<RenderEngine>,
// }
// 
// impl Drop for DescriptorSetLayoutResource {
//     fn drop(&mut self) {
//         unsafe {
//             self.engine
//                 .device
//                 .destroy_descriptor_set_layout(self.layout, None);
//         }
//     }
// }
