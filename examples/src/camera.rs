use bevy_app::{App, Plugin, PostUpdate, Startup};
use bevy_ecs::{
    component::Component,
    query::With,
    schedule::IntoScheduleConfigs,
    system::{Commands, Query, Res},
};
use bevy_time::Time;
use bevy_transform::{TransformSystems, components::Transform};
use bevy_window::{PrimaryWindow, Window};
use lantir_render::{bevy::components::Camera, scene::CameraTransform};

use crate::Car;

#[derive(Component)]
pub struct ChaseCameraController {
    pub position: glam::Vec3,
    pub follow_distance: f32,
    pub follow_height: f32,
    pub look_height: f32,
    pub smoothing: f32,
}

impl Default for ChaseCameraController {
    fn default() -> Self {
        Self {
            position: glam::Vec3::new(0.0, 10.0, 30.0),
            follow_distance: 12.0,
            follow_height: 4.0,
            look_height: 1.5,
            smoothing: 10.0,
        }
    }
}

pub struct ChaseCameraPlugin;

impl Plugin for ChaseCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_chase_camera).add_systems(
            PostUpdate,
            update_chase_camera.before(TransformSystems::Propagate),
        );
    }
}

fn spawn_chase_camera(mut commands: Commands) {
    let controller = ChaseCameraController::default();
    let camera = build_camera_transform(controller.position, controller.position, 16.0 / 9.0);
    commands.spawn((controller, Camera(camera)));
}

fn build_camera_transform(
    position: glam::Vec3,
    target: glam::Vec3,
    aspect: f32,
) -> CameraTransform {
    let view = glam::Mat4::look_at_rh(position, target, glam::Vec3::Y);
    let mut proj = glam::Mat4::perspective_infinite_reverse_rh(70f32.to_radians(), aspect, 0.1);
    proj.y_axis.y *= -1.0;

    let viewproj = proj * view;
    CameraTransform {
        view,
        proj,
        viewproj,
        inv_viewproj: viewproj.inverse(),
        camera_pos: position.extend(1.0),
    }
}

fn update_chase_camera(
    time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>,
    car_query: Query<&Transform, With<Car>>,
    mut camera_query: Query<(&mut ChaseCameraController, &mut Camera)>,
) {
    let Some(car_transform) = car_query.iter().next() else {
        return;
    };
    let Some((mut controller, mut camera_component)) = camera_query.iter_mut().next() else {
        return;
    };

    let dt = time.delta_secs().min(0.05);
    let aspect = if let Some(win) = windows.iter().next() {
        let w = win.physical_width();
        let h = win.physical_height();
        if h > 0 && w > 0 {
            w as f32 / h as f32
        } else {
            16.0 / 9.0
        }
    } else {
        16.0 / 9.0
    };

    let car_pos = car_transform.translation;
    let car_forward = car_transform
        .rotation
        .mul_vec3(glam::Vec3::new(0.0, 0.0, 1.0))
        .normalize_or_zero();

    let desired = car_pos + glam::Vec3::Y * controller.follow_height
        - car_forward * controller.follow_distance;

    // Exponential smoothing for frame-rate independent camera motion.
    let t = 1.0 - (-controller.smoothing * dt).exp();
    controller.position = controller.position.lerp(desired, t);

    let target = car_pos + glam::Vec3::Y * controller.look_height;
    *camera_component = Camera(build_camera_transform(controller.position, target, aspect));
}
