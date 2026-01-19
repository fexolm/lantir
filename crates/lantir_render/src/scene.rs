use crate::resources::DrawItem;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct CameraTransform {
    pub view: glam::Mat4,
    pub proj: glam::Mat4,
    pub viewproj: glam::Mat4,
    pub inv_viewproj: glam::Mat4,
    pub camera_pos: glam::Vec4,
}
pub struct Scene<'i> {
    pub camera: CameraTransform,
    pub draw_items: &'i [DrawItem],
}
