use bevy_app::{App, Plugin, Update};
use bevy_ecs::{event::EventReader, query::With, system::{Query, Res, ResMut, Resource}};
use bevy_input::{
    ButtonInput, keyboard::KeyCode, mouse::{MouseButton, MouseMotion, MouseWheel}
};
use bevy_window::{PrimaryWindow, Window};
use lantir_render::scene::CameraTransform;
use std::time::Instant;

#[derive(Resource, Clone, Copy)]
pub struct SpectatorCamera {
    pub position: glam::Vec3,
    pub yaw: f32,
    pub pitch: f32,

    pub speed: f32,
    pub sprint_mul: f32,
    pub mouse_sensitivity: f32,

    last_update: Instant,

    camera: CameraTransform,
}

impl Default for SpectatorCamera {
    fn default() -> Self {
        let position = glam::Vec3::new(0.0, 50.0, 300.0);
        let yaw = 0.0;
        let pitch = 0.0;

        let view = glam::Mat4::look_at_rh(position, position + glam::Vec3::X, glam::Vec3::Y);
        // Minimal fixed projection; aspect will be "good enough" for the example.
        let mut proj = glam::Mat4::perspective_rh(70f32.to_radians(), 16.0 / 9.0, 0.1, 10000.0);
        proj.y_axis.y *= -1.0;

        let camera = CameraTransform {
            view,
            proj,
            viewproj: proj * view,
        };

        Self {
            position,
            yaw,
            pitch,
            speed: 250.0,
            sprint_mul: 3.0,
            mouse_sensitivity: 0.0025,
            last_update: Instant::now(),
            camera,
        }
    }
}

impl SpectatorCamera {
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

    fn rebuild_camera(&mut self) {
        let forward = self.forward();
        let view = glam::Mat4::look_at_rh(self.position, self.position + forward, glam::Vec3::Y);
        self.camera.view = view;
        self.camera.viewproj = self.camera.proj * view;
    }

    pub fn get_transform(&self) -> CameraTransform {
        self.camera
    }
}

#[derive(Default)]
pub struct SpectatorCameraPlugin {
}

impl Plugin for SpectatorCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpectatorCamera>()
            .add_systems(Update, update_spectator_camera);
    }
}

fn update_spectator_camera(
    mut camera: ResMut<SpectatorCamera>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut mouse_motion_events: EventReader<MouseMotion>,
    mut mouse_wheel_events: EventReader<MouseWheel>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if mouse_input.pressed(MouseButton::Right) {
        for ev in mouse_motion_events.read() {
            camera.yaw += ev.delta.x * camera.mouse_sensitivity;
            camera.pitch -= ev.delta.y * camera.mouse_sensitivity;
        }
    } else {
        mouse_motion_events.clear();
    }
    camera.pitch = camera.pitch.clamp(-1.55, 1.55);

    for ev in mouse_wheel_events.read() {
        camera.speed = (camera.speed * (1.0 + ev.y * 0.1)).clamp(10.0, 50_000.0);
    }

    let now = Instant::now();
    let dt = (now - camera.last_update).as_secs_f32().min(0.1);
    camera.last_update = now;

    let mut dir = glam::Vec3::ZERO;
    if keyboard_input.pressed(KeyCode::KeyW) { dir += camera.forward(); }
    if keyboard_input.pressed(KeyCode::KeyS) { dir -= camera.forward(); }
    if keyboard_input.pressed(KeyCode::KeyD) { dir += camera.right(); }
    if keyboard_input.pressed(KeyCode::KeyA) { dir -= camera.right(); }
    if keyboard_input.pressed(KeyCode::Space) { dir += glam::Vec3::Y; }
    if keyboard_input.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
        dir -= glam::Vec3::Y;
    }

    if dir.length_squared() > 0.0 {
        let mut speed = camera.speed;
        if keyboard_input.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
            speed *= camera.sprint_mul;
        }
        camera.position += dir.normalize() * speed * dt;
    }

    if let Ok(win) = windows.get_single() {
        let w = win.physical_width();
        let h = win.physical_height();
        if h > 0 && w > 0 {
            let aspect = w as f32 / h as f32;
            let mut proj = glam::Mat4::perspective_rh(70f32.to_radians(), aspect, 0.1, 10000.0);
            proj.y_axis.y *= -1.0;
            camera.camera.proj = proj;
        }
    }

    camera.rebuild_camera();
}
