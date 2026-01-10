use crate::resources::DrawItem;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct Camera {
    pub view: glam::Mat4,
    pub proj: glam::Mat4,
    pub viewproj: glam::Mat4,
}
pub struct Scene<'i> {
    pub camera: Camera,
    pub draw_items: &'i [DrawItem],
}
