use image::DynamicImage;
use lantir_render::resources::{DrawItem, INVALID_RESOURCE_HANDLE, PbrMaterial, TriMesh, Vertex};
use lantir_render::scene::{Camera, Scene};
use lantir_render::world_renderer::{self, WorldRenderer, WorldRendererConfig};
use lantir_hal::{AccessType, Buffer, CopyBufferImageInfo, ImageBarrier, RenderEngine, RenderEngineConfig, Texture, TextureCreateInfo, UpdateFrequency, vk};
use std::collections::HashMap;
use std::time::Instant;
use winit::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowBuilder};

const WINDOW_WIDTH: u32 = 1300;
const WINDOW_HEIGHT: u32 = 900;

#[derive(Debug, Clone, Copy, Default)]
struct CameraInput {
    mouse_pressed: bool,
    last_cursor_pos: Option<(f64, f64)>,
    pending_mouse_delta: (f32, f32),
    pending_scroll: f32,

    key_left: bool,
    key_right: bool,
    key_up: bool,
    key_down: bool,

    key_w: bool,
    key_a: bool,
    key_s: bool,
    key_d: bool,
}

impl CameraInput {
    fn set_key(&mut self, key: KeyCode, pressed: bool) {
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

    fn add_mouse_delta(&mut self, dx: f32, dy: f32) {
        self.pending_mouse_delta.0 += dx;
        self.pending_mouse_delta.1 += dy;
    }

    fn add_scroll(&mut self, scroll: f32) {
        self.pending_scroll += scroll;
    }

    fn take_mouse_delta(&mut self) -> (f32, f32) {
        let delta = self.pending_mouse_delta;
        self.pending_mouse_delta = (0.0, 0.0);
        delta
    }

    fn take_scroll(&mut self) -> f32 {
        let scroll = self.pending_scroll;
        self.pending_scroll = 0.0;
        scroll
    }
}

#[derive(Debug, Clone, Copy)]
struct OrbitCamera {
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

impl OrbitCamera {
    fn update(&mut self, input: &mut CameraInput, dt: f32) {
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

    fn view_matrix(&self) -> glam::Mat4 {
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

struct App {
    window: Window,
    world_renderer: WorldRenderer,

    draw_items: Vec<DrawItem>,

    camera: OrbitCamera,
    camera_input: CameraInput,
    last_frame_time: Instant,
}

fn load_gltf_draw_items(
    world_renderer: &WorldRenderer,
    gltf_bytes: &[u8],
) -> anyhow::Result<Vec<DrawItem>> {
    let rm = world_renderer.get_resource_manager();
    let gltf = gltf::Gltf::from_slice(gltf_bytes)?;

    // Enforce GLB-only: no external buffers via URI.
    for buffer in gltf.buffers() {
        if matches!(buffer.source(), gltf::buffer::Source::Uri(_)) {
            anyhow::bail!(
                "External buffers are not supported anymore; provide a .glb with embedded BIN chunk"
            );
        }
    }

    fn decode_gltf_image(
        gltf: &gltf::Gltf,
        image: &gltf::Image,
    ) -> anyhow::Result<DynamicImage> {
        let bytes: Vec<u8> = match image.source() {
            gltf::image::Source::View { view, .. } => {
                let buffer = view.buffer();
                let Some(blob) = gltf.blob.as_deref() else {
                    anyhow::bail!("Missing GLB BIN blob");
                };
                match buffer.source() {
                    gltf::buffer::Source::Bin => {
                        let start = view.offset();
                        let end = start + view.length();
                        blob.get(start..end)
                            .ok_or_else(|| anyhow::anyhow!("Image buffer view out of bounds"))?
                            .to_vec()
                    }
                    gltf::buffer::Source::Uri(_) => {
                        anyhow::bail!("External image buffers are not supported (GLB-only)")
                    }
                }
            }
            gltf::image::Source::Uri { uri, .. } => {
                // For GLB-only workflow we disallow external files.
                // If you still have a data URI, it can be supported later.
                anyhow::bail!("Image URI is not supported (expected embedded image in .glb): {uri}")
            }
        };

        let dyn_img = image::load_from_memory(&bytes)?;
        Ok(dyn_img)
    }

    // Preload images -> GPU textures, and remember mapping.
    let mut image_to_texture: HashMap<usize, lantir_render::resources::TextureHandle> = HashMap::new();
    for img in gltf.images() {
        let texture = decode_gltf_image(&gltf, &img)?;
        let handle = rm.add_texture(texture)?;
        image_to_texture.insert(img.index(), handle);
    }

    fn node_transform(node: &gltf::Node) -> glam::Mat4 {
        match node.transform() {
            gltf::scene::Transform::Matrix { matrix } => glam::Mat4::from_cols_array_2d(&matrix),
            gltf::scene::Transform::Decomposed {
                translation,
                rotation,
                scale,
            } => {
                let t = glam::Mat4::from_translation(glam::Vec3::from(translation));
                let r = glam::Mat4::from_quat(glam::Quat::from_array(rotation));
                let s = glam::Mat4::from_scale(glam::Vec3::from(scale));
                t * r * s
            }
        }
    }

    fn load_node(
        node: gltf::Node,
        gltf: &gltf::Gltf,
        parent_transform: glam::Mat4,
        rm: &lantir_render::resources::resource_manager::ResourceManager,
        image_to_texture: &HashMap<usize, lantir_render::resources::TextureHandle>,
        draw_items: &mut Vec<DrawItem>,
    ) -> anyhow::Result<()> {
        let world_transform = parent_transform * node_transform(&node);

        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                let reader = primitive.reader(|buffer| match buffer.source() {
                    gltf::buffer::Source::Bin => gltf.blob.as_deref(),
                    gltf::buffer::Source::Uri(_uri) => None,
                });

                let Some(positions) = reader.read_positions() else {
                    continue;
                };
                let positions: Vec<[f32; 3]> = positions.collect();

                let normals: Vec<[f32; 3]> = reader
                    .read_normals()
                    .map(|it| it.collect())
                    .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

                let tex_coords: Vec<[f32; 2]> = reader
                    .read_tex_coords(0)
                    .map(|it| it.into_f32().collect())
                    .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

                let indices: Vec<u32> = reader
                    .read_indices()
                    .map(|it| it.into_u32().collect())
                    .unwrap_or_else(|| (0..positions.len() as u32).collect());

                let mut vertices = Vec::with_capacity(positions.len());
                for i in 0..positions.len() {
                    vertices.push(Vertex {
                        position: glam::Vec3::from(positions[i]),
                        normal: glam::Vec3::from(normals[i]),
                        color: glam::Vec4::ONE,
                        uv: glam::Vec2::from(tex_coords[i]),
                    });
                }

                let mesh_handle = rm.add_mesh(&TriMesh { vertices, indices })?;

                let pbr = primitive.material().pbr_metallic_roughness();
                let base_color = glam::Vec4::from_array(pbr.base_color_factor());

                let (albedo_tex, albedo_sampler) = if let Some(info) = pbr.base_color_texture() {
                    let img_index = info.texture().source().index();
                    let handle = image_to_texture
                        .get(&img_index)
                        .copied()
                        .unwrap_or(INVALID_RESOURCE_HANDLE);
                    // ResourceManager seeds sampler[0] with a default linear sampler.
                    (handle, 0)
                } else {
                    (INVALID_RESOURCE_HANDLE, INVALID_RESOURCE_HANDLE)
                };

                let material_handle = rm.add_material(PbrMaterial {
                    albedo_tex,
                    albedo_sampler,
                    normal_tex: INVALID_RESOURCE_HANDLE,
                    normal_sampler: INVALID_RESOURCE_HANDLE,
                    metallic_roughness_tex: INVALID_RESOURCE_HANDLE,
                    metallic_roughness_sampler: INVALID_RESOURCE_HANDLE,
                    emissive_tex: INVALID_RESOURCE_HANDLE,
                    emissive_sampler: INVALID_RESOURCE_HANDLE,
                    base_color,
                    emissive_color: glam::Vec3::ZERO,
                    metallness: pbr.metallic_factor(),
                    roughness: pbr.roughness_factor(),
                })?;

                draw_items.push(DrawItem {
                    transform: world_transform,
                    mesh: mesh_handle,
                    material: material_handle,
                });
            }
        }

        for child in node.children() {
            load_node(
                child,
                gltf,
                world_transform,
                rm,
                image_to_texture,
                draw_items,
            )?;
        }

        Ok(())
    }

    let mut draw_items = Vec::new();
    for scene in gltf.scenes() {
        for node in scene.nodes() {
            load_node(
                node,
                &gltf,
                glam::Mat4::IDENTITY,
                rm,
                &image_to_texture,
                &mut draw_items,
            )?;
        }
    }

    Ok(draw_items)
}

impl App {
    fn new(event_loop: &EventLoop<()>) -> anyhow::Result<Self> {
        let window = WindowBuilder::new()
            .with_title("Example App")
            .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
            .build(&event_loop)?;

        let image_extent = vk::Extent2D {
            width: window.inner_size().width,
            height: window.inner_size().height,
        };

        let draw_extent = image_extent;

        let engine = {
            let config = RenderEngineConfig {
                debug: true,
                frames_in_flight: 2,
            };
            RenderEngine::new(&window, &config)?
        };

        let world_renderer = world_renderer::WorldRenderer::new(
            engine.clone(),
            &WorldRendererConfig {
                draw_extent,
                window_extent: draw_extent,
            },
        )?;

        let draw_items = load_gltf_draw_items(
            &world_renderer,
            include_bytes!("../assets/track.glb"),
        )?;

        Ok(App {
            window,
            world_renderer,
            draw_items,
            camera: OrbitCamera::default(),
            camera_input: CameraInput::default(),
            last_frame_time: Instant::now(),
        })
    }

    pub fn run(&mut self, event_loop: EventLoop<()>) {
        event_loop
            .run(move |ev, target| match ev {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::RedrawRequested => self.draw_frame(),
                    WindowEvent::CloseRequested => target.exit(),
                    WindowEvent::Resized(size) => {
                        self.world_renderer
                            .resize(vk::Extent2D {
                                width: size.width,
                                height: size.height,
                            })
                            .unwrap();
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        if let PhysicalKey::Code(code) = event.physical_key {
                            self.camera_input
                                .set_key(code, event.state == ElementState::Pressed);
                        }
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        if button == MouseButton::Left {
                            self.camera_input.mouse_pressed = state == ElementState::Pressed;
                            if !self.camera_input.mouse_pressed {
                                self.camera_input.last_cursor_pos = None;
                            }
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        if self.camera_input.mouse_pressed {
                            let (x, y) = (position.x, position.y);
                            if let Some((lx, ly)) = self.camera_input.last_cursor_pos {
                                self.camera_input
                                    .add_mouse_delta((x - lx) as f32, (y - ly) as f32);
                            }
                            self.camera_input.last_cursor_pos = Some((x, y));
                        }
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        let scroll = match delta {
                            MouseScrollDelta::LineDelta(_x, y) => y,
                            MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 120.0,
                        };
                        self.camera_input.add_scroll(scroll);
                    }
                    _ => (),
                },
                Event::AboutToWait => self.window.request_redraw(),
                _ => (),
            })
            .unwrap();
    }

    fn update_camera(&mut self) -> glam::Mat4 {
        let now = Instant::now();
        let dt = (now - self.last_frame_time).as_secs_f32().min(0.1);
        self.last_frame_time = now;

        self.camera.update(&mut self.camera_input, dt);
        self.camera.view_matrix()
    }

    fn draw_frame(&mut self) {
        let view = self.update_camera();

        let draw_extent = self.world_renderer.draw_extent();

        let mut proj: glam::Mat4 = glam::Mat4::perspective_rh(
            70f32.to_radians(),
            draw_extent.width as f32 / draw_extent.height as f32,
            0.1,
            10000.0,
        );

        proj.y_axis.y *= -1.0;

        let camera = Camera {
            view,
            proj,
            viewproj: proj * view,
        };

        let scene = Scene {
            camera,
            draw_items: &self.draw_items,
        };

        self.world_renderer.draw_frame(&scene).unwrap();
    }
}

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = App::new(&event_loop)?;
    app.run(event_loop);
    Ok(())
}
