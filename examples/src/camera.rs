use bevy_ecs::system::Resource;
use bevy_input::keyboard::KeyCode;
use lantir_render::scene::Camera;

#[derive(Debug, Clone, Copy, Default, Resource)]
pub struct CameraInput {
    pub mouse_pressed: bool,
    pub last_cursor_pos: Option<(f32, f32)>,
    pub pending_mouse_delta: (f32, f32),
    pub pending_scroll: f32,

    pub key_left: bool,
    pub key_right: bool,
    pub key_up: bool,
    pub key_down: bool,

    pub key_w: bool,
    pub key_a: bool,
    pub key_s: bool,
    pub key_d: bool,
}

impl CameraInput {
    pub fn set_key(&mut self, key: KeyCode, pressed: bool) {
        match key {
            KeyCode::ArrowLeft => self.key_left = pressed,
            KeyCode::ArrowRight => self.key_right = pressed,
            KeyCode::ArrowUp => self.key_up = pressed,
            KeyCode::ArrowDown => self.key_down = pressed,
            KeyCode::KeyW => self.key_w = pressed,
            KeyCode::KeyA => self.key_a = pressed,
            KeyCode::KeyS => self.key_s = pressed,
            KeyCode::KeyD => self.key_d = pressed,
            _ => {}
        }
    }

    pub fn add_mouse_delta(&mut self, dx: f32, dy: f32) {
        self.pending_mouse_delta.0 += dx;
        self.pending_mouse_delta.1 += dy;
    }

    pub fn add_scroll(&mut self, scroll: f32) {
        self.pending_scroll += scroll;
    }

    pub fn take_mouse_delta(&mut self) -> (f32, f32) {
        let delta = self.pending_mouse_delta;
        self.pending_mouse_delta = (0.0, 0.0);
        delta
    }

    pub fn take_scroll(&mut self) -> f32 {
        let scroll = self.pending_scroll;
        self.pending_scroll = 0.0;
        scroll
    }
}

#[derive(Debug, Clone, Copy, Resource)]
pub struct OrbitCamera {
    target: glam::Vec3,
    radius: f32,
    yaw: f32,
    pitch: f32,

    rotate_sensitivity: f32,
    key_rotate_speed: f32,
    zoom_sensitivity: f32,
    move_speed: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: glam::Vec3::ZERO,
            radius: 500.0,
            yaw: 0.0,
            pitch: 0.2,
            rotate_sensitivity: 0.005,
            key_rotate_speed: 1.5,
            zoom_sensitivity: 0.12,
            move_speed: 250.0,
        }
    }
}

#[derive(Resource)]
pub struct CameraState {
    pub camera: Camera,
}

impl OrbitCamera {
    pub fn update(&mut self, input: & CameraInput, dt: f32) {
        let (dx, dy) = input.take_mouse_delta();
        self.yaw += dx * self.rotate_sensitivity;
        self.pitch += -dy * self.rotate_sensitivity;

        let key_yaw = (input.key_right as i32 - input.key_left as i32) as f32;
        let key_pitch = (input.key_up as i32 - input.key_down as i32) as f32;
        self.yaw += key_yaw * self.key_rotate_speed * dt;
        self.pitch += key_pitch * self.key_rotate_speed * dt;

        let scroll = input.take_scroll();
        if scroll != 0.0 {
            let zoom_factor = 1.0 - scroll * self.zoom_sensitivity;
            self.radius = (self.radius * zoom_factor).clamp(0.5, 50_000.0);
        }

        // WASD pan: move target in the camera's local XZ plane
        let move_x = (input.key_d as i32 - input.key_a as i32) as f32;
        let move_z = (input.key_w as i32 - input.key_s as i32) as f32;
        if move_x != 0.0 || move_z != 0.0 {
            let cp = self.pitch.cos();
            let sp = self.pitch.sin();
            let cy = self.yaw.cos();
            let sy = self.yaw.sin();

            let camera_offset = glam::vec3(
                self.radius * cp * cy,
                self.radius * sp,
                self.radius * cp * sy,
            );
            let camera_position = self.target + camera_offset;
            let forward = (self.target - camera_position)
                .try_normalize()
                .unwrap_or(glam::Vec3::Z);
            let right = forward
                .cross(glam::Vec3::Y)
                .try_normalize()
                .unwrap_or(glam::Vec3::X);

            let planar_forward = glam::vec3(forward.x, 0.0, forward.z)
                .try_normalize()
                .unwrap_or(glam::Vec3::Z);

            let move_dir = (right * move_x + planar_forward * move_z)
                .try_normalize()
                .unwrap_or(glam::Vec3::ZERO);
            let speed = (self.move_speed * (self.radius * 0.002).max(1.0)) * dt;
            self.target += move_dir * speed;
        }

        self.pitch = self.pitch.clamp(-1.55, 1.55);

        if self.yaw.abs() > 1000.0 {
            self.yaw = self.yaw.rem_euclid(std::f32::consts::TAU);
        }
    }

    pub fn view_matrix(&self) -> glam::Mat4 {
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();

        let camera_offset = glam::vec3(
            self.radius * cp * cy,
            self.radius * sp,
            self.radius * cp * sy,
        );
        let camera_position = self.target + camera_offset;
        glam::Mat4::look_at_rh(camera_position, self.target, glam::Vec3::Y)
    }
}