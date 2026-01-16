use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::{
    component::Component,
    message::MessageReader,
    query::With,
    system::{Commands, Query, Res},
};
use bevy_input::{
    ButtonInput,
    keyboard::KeyCode,
    mouse::{MouseButton, MouseMotion, MouseWheel},
};
use bevy_window::{PrimaryWindow, Window};
use lantir_render::{bevy::components::Camera, scene::CameraTransform};
use std::time::Instant;

#[derive(Component)]
pub struct SpectatorCameraController {
    pub position: glam::Vec3,
    pub yaw: f32,
    pub pitch: f32,

    pub speed: f32,
    pub sprint_mul: f32,
    pub mouse_sensitivity: f32,

    last_update: Instant,
}

impl Default for SpectatorCameraController {
    fn default() -> Self {
        let position = glam::Vec3::new(0.0, 50.0, 300.0);
        // Look roughly towards the origin by default.
        let yaw = -std::f32::consts::FRAC_PI_2;
        let pitch = -0.15;

        Self {
            position,
            yaw,
            pitch,
            speed: 250.0,
            sprint_mul: 3.0,
            mouse_sensitivity: 0.0025,
            last_update: Instant::now(),
        }
    }
}

impl SpectatorCameraController {
    fn forward(&self) -> glam::Vec3 {
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        glam::Vec3::new(cy * cp, sp, sy * cp).normalize_or_zero()
    }

    fn right(&self) -> glam::Vec3 {
        self.forward().cross(glam::Vec3::Y).normalize_or_zero()
    }

    fn camera_transform(&self, aspect: f32) -> CameraTransform {
        let forward = self.forward();
        let view = glam::Mat4::look_at_rh(self.position, self.position + forward, glam::Vec3::Y);
        let mut proj = glam::Mat4::perspective_rh(70f32.to_radians(), aspect, 0.1, 10000.0);
        proj.y_axis.y *= -1.0;
        CameraTransform {
            view,
            proj,
            viewproj: proj * view,
        }
    }
}

pub struct SpectatorCameraPlugin;

impl Plugin for SpectatorCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_spectator_camera)
            .add_systems(Update, update_spectator_camera);
    }
}

fn spawn_spectator_camera(mut commands: Commands) {
    let controller = SpectatorCameraController::default();
    let camera = controller.camera_transform(16.0 / 9.0);
    commands.spawn((controller, Camera(camera)));
}

fn update_spectator_camera(
    mut query: Query<(&mut SpectatorCameraController, &mut Camera)>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut mouse_motion_events: MessageReader<MouseMotion>,
    mut mouse_wheel_events: MessageReader<MouseWheel>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok((mut controller, mut camera_component)) = query.single_mut() else {
        return;
    };

    if mouse_input.pressed(MouseButton::Right) {
        for ev in mouse_motion_events.read() {
            controller.yaw += ev.delta.x * controller.mouse_sensitivity;
            controller.pitch -= ev.delta.y * controller.mouse_sensitivity;
        }
    } else {
        mouse_motion_events.clear();
    }
    controller.pitch = controller.pitch.clamp(-1.55, 1.55);

    for ev in mouse_wheel_events.read() {
        controller.speed = (controller.speed * (1.0 + ev.y * 0.1)).clamp(10.0, 50_000.0);
    }

    let now = Instant::now();
    let dt = (now - controller.last_update).as_secs_f32().min(0.1);
    controller.last_update = now;

    let mut dir = glam::Vec3::ZERO;
    if keyboard_input.pressed(KeyCode::KeyW) {
        dir += controller.forward();
    }
    if keyboard_input.pressed(KeyCode::KeyS) {
        dir -= controller.forward();
    }
    if keyboard_input.pressed(KeyCode::KeyD) {
        dir += controller.right();
    }
    if keyboard_input.pressed(KeyCode::KeyA) {
        dir -= controller.right();
    }
    if keyboard_input.pressed(KeyCode::Space) {
        dir += glam::Vec3::Y;
    }
    if keyboard_input.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
        dir -= glam::Vec3::Y;
    }

    if dir.length_squared() > 0.0 {
        let mut speed = controller.speed;
        if keyboard_input.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
            speed *= controller.sprint_mul;
        }
        controller.position += dir.normalize() * speed * dt;
    }

    let aspect = if let Ok(win) = windows.single() {
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

    *camera_component = Camera(controller.camera_transform(aspect));
}
