use crate::{
    resources::{MaterialHandle, MeshHandle},
    scene::CameraTransform,
};
use bevy_ecs::component::Component;

#[derive(Component)]
pub struct Mesh(pub MeshHandle);

#[derive(Component)]
pub struct Material(pub MaterialHandle);

#[derive(Component)]
pub struct Transform(pub glam::Mat4);

#[derive(Component)]
pub struct Camera(pub CameraTransform);
