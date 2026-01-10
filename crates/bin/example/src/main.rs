mod render_object;

use lantir::scene::{Camera, Scene};
use lantir::world_renderer::{self, WorldRenderer, WorldRendererConfig};
use lantir_hal::{RenderEngine, RenderEngineConfig, vk};
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

    camera: OrbitCamera,
    camera_input: CameraInput,
    last_frame_time: Instant,
}

// fn load_texture(
//     engine: Arc<RenderEngine>,
//     data: &[u8],
//     extent: vk::Extent3D,
//     format: vk::Format,
//     usage: vk::ImageUsageFlags,
//     mip_levels: u32,
// ) -> anyhow::Result<Texture> {
//     let staging_buffer = Buffer::new(
//         engine.clone(),
//         &BufferCreateInfo {
//             size: data.len() as u64,
//             update_frequency: UpdateFrequency::Static,
//             memory_property: vk::MemoryPropertyFlags::HOST_VISIBLE
//                 | vk::MemoryPropertyFlags::HOST_COHERENT,
//             vma_flags: AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
//             usage: vk::BufferUsageFlags::TRANSFER_SRC,
//         },
//     )?;

//     unsafe {
//         let staging_buffer_map = staging_buffer.map()?;
//         std::ptr::copy_nonoverlapping(data.as_ptr() as *const u8, staging_buffer_map, data.len());
//         staging_buffer.unmap();
//     }

//     let texture = Texture::new(
//         engine.clone(),
//         &TextureCreateInfo {
//             image_type: ImageType::TYPE_2D,
//             update_frequency: UpdateFrequency::Static,
//             format,
//             extent,
//             usage: usage | vk::ImageUsageFlags::TRANSFER_DST,
//             aspect: vk::ImageAspectFlags::COLOR,
//             mip_levels,
//         },
//     )?;

//     engine.immediate_submit(|cb| {
//         cb.cmd_image_barrier(
//             &engine,
//             &ImageBarrier {
//                 previous_accesses: &[AccessType::Nothing],
//                 next_accesses: &[AccessType::TransferWrite],
//                 previous_layout: vk::ImageLayout::UNDEFINED,
//                 next_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
//                 image: &texture,
//                 aspect_mask: vk::ImageAspectFlags::COLOR,
//             },
//         );

//         cb.cmd_copy_buffer_to_image(
//             &engine,
//             &CopyBufferImageInfo {
//                 buffer: &staging_buffer,
//                 image: &texture,
//                 image_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
//                 image_aspect_mask: vk::ImageAspectFlags::COLOR,
//                 image_extent: extent,
//             },
//         );

//         cb.cmd_image_barrier(
//             &engine,
//             &ImageBarrier {
//                 previous_accesses: &[AccessType::TransferWrite],
//                 next_accesses: &[AccessType::FragmentShaderReadSampledImageOrUniformTexelBuffer],
//                 previous_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
//                 next_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
//                 image: &texture,
//                 aspect_mask: vk::ImageAspectFlags::COLOR,
//             },
//         );
//     })?;

//     Ok(texture)
// }

// fn load_glfw(
//     engine: Arc<RenderEngine>,
//     gltf_bytes: &[u8],
//     external_buffers: &[(&str, &[u8])],
// ) -> anyhow::Result<Scene> {
//     let gltf = Gltf::from_slice(gltf_bytes)?;

//     let mut cnt = 0;

//     let mr_mat = MetallicRoughnessMat::new(engine.clone())?;

//     let mut textures = Vec::new();
//     let mut samplers = Vec::new();
//     let mut objects = Vec::new();
//     let mut buffers = Vec::new();

//     static WHITE_TEXTURE_DATA: [u8; 16] = [
//         255, 255, 255, 255, // Пиксель 1
//         255, 255, 255, 255, // Пиксель 2
//         255, 255, 255, 255, // Пиксель 3
//         255, 255, 255, 255, // Пиксель 4
//     ];

//     let white_texture = load_texture(
//         engine.clone(),
//         &WHITE_TEXTURE_DATA,
//         vk::Extent3D {
//             width: 2,
//             height: 2,
//             depth: 1,
//         },
//         vk::Format::B8G8R8A8_UNORM,
//         vk::ImageUsageFlags::SAMPLED,
//         1,
//     )?;

//     let sampler = Sampler::new(
//         engine.clone(),
//         &SamplerInfo {
//             filter: vk::Filter::LINEAR,
//         },
//     )?;

//     fn load_node_meshes(
//         node: gltf::Node,
//         gltf: &Gltf,
//         engine: Arc<RenderEngine>,
//         mr_mat: &Arc<MetallicRoughnessMat>,
//         white_texture: &Texture,
//         sampler: &Sampler,
//         external_buffers: &[(&str, &[u8])],
//         parent_transform: glam::Mat4,
//         objects: &mut Vec<RenderObject>,
//         buffers: &mut Vec<Buffer>,
//         cnt: &mut usize,
//     ) -> anyhow::Result<()> {
//         let local_transform = match node.transform() {
//             gltf::scene::Transform::Matrix { matrix } => glam::Mat4::from_cols_array_2d(&matrix),
//             gltf::scene::Transform::Decomposed {
//                 translation,
//                 rotation,
//                 scale,
//             } => {
//                 let t = glam::Mat4::from_translation(glam::Vec3::from(translation));
//                 let r = glam::Mat4::from_quat(glam::Quat::from_array(rotation));
//                 let s = glam::Mat4::from_scale(glam::Vec3::from(scale));
//                 t * r * s
//             }
//         };

//         let world_transform = parent_transform * local_transform;

//         if let Some(mesh) = node.mesh() {
//             for primitive in mesh.primitives() {
//                 let reader = primitive.reader(|buffer| match buffer.source() {
//                     gltf::buffer::Source::Bin => gltf.blob.as_deref(),
//                     gltf::buffer::Source::Uri(uri) => external_buffers
//                         .iter()
//                         .find_map(|(name, bytes)| (*name == uri).then_some(*bytes)),
//                 });

//                 let Some(positions) = reader.read_positions() else {
//                     continue;
//                 };

//                 let positions: Vec<_> = positions.collect();

//                 let normals: Vec<[f32; 3]> = reader
//                     .read_normals()
//                     .ok_or_else(|| anyhow::anyhow!("No normals in mesh"))?
//                     .collect();

//                 let tex_coords: Vec<[f32; 2]> = if let Some(tex_coords) = reader.read_tex_coords(0)
//                 {
//                     tex_coords.into_f32().collect()
//                 } else {
//                     vec![[0.0, 0.0]; positions.len()]
//                 };

//                 let indices: Vec<u32> = reader
//                     .read_indices()
//                     .ok_or_else(|| anyhow::anyhow!("No indices in mesh"))?
//                     .into_u32()
//                     .collect();

//                 let mut vertices = Vec::new();

//                 for i in 0..positions.len() {
//                     vertices.push(Vertex {
//                         position: glam::Vec3::from(positions[i]),
//                         normal: glam::Vec3::from(normals[i]),
//                         color: glam::Vec4::ONE,
//                         uv: glam::Vec2::from(tex_coords[i]),
//                     });
//                 }

//                 let constants_buf = Buffer::new(
//                     engine.clone(),
//                     &BufferCreateInfo {
//                         size: size_of::<MaterialConstants>() as u64,
//                         update_frequency: UpdateFrequency::PerFrame,
//                         memory_property: vk::MemoryPropertyFlags::HOST_VISIBLE
//                             | vk::MemoryPropertyFlags::HOST_COHERENT,
//                         vma_flags: AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
//                         usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
//                     },
//                 )?;

//                 let mesh = crate::render_object::load_mesh(engine.clone(), &vertices, &indices)?;
//                 let material = mr_mat.new_instance(engine.clone())?;

//                 *cnt += 1;

//                 let constants = MaterialConstants {
//                     color_factors: glam::Vec4::from_array(
//                         primitive
//                             .material()
//                             .pbr_metallic_roughness()
//                             .base_color_factor(),
//                     ),
//                     metal_rough_factors: glam::Vec4::new(
//                         primitive
//                             .material()
//                             .pbr_metallic_roughness()
//                             .metallic_factor(),
//                         primitive
//                             .material()
//                             .pbr_metallic_roughness()
//                             .roughness_factor(),
//                         0.,
//                         0.,
//                     ),
//                     extra: [Default::default(); 14],
//                 };

//                 unsafe {
//                     let data = constants_buf.map()?;
//                     std::ptr::copy_nonoverlapping(
//                         (&constants) as *const MaterialConstants as *const u8,
//                         data,
//                         size_of::<MaterialConstants>(),
//                     );
//                     constants_buf.unmap();
//                 }

//                 material.set_material_constants(&constants_buf, 0);
//                 material.set_color_image(&white_texture, &sampler);
//                 material.set_metal_rough_image(&white_texture, &sampler);

//                 buffers.push(constants_buf);

//                 objects.push(RenderObject {
//                     mesh,
//                     material,
//                     transform: world_transform,
//                 });
//             }
//         }

//         for child in node.children() {
//             load_node_meshes(
//                 child,
//                 gltf,
//                 engine.clone(),
//                 mr_mat,
//                 white_texture,
//                 sampler,
//                 external_buffers,
//                 world_transform,
//                 objects,
//                 buffers,
//                 cnt,
//             )?;
//         }

//         Ok(())
//     }

//     for scene in gltf.scenes() {
//         for node in scene.nodes() {
//             load_node_meshes(
//                 node,
//                 &gltf,
//                 engine.clone(),
//                 &mr_mat,
//                 &white_texture,
//                 &sampler,
//                 external_buffers,
//                 glam::Mat4::IDENTITY,
//                 &mut objects,
//                 &mut buffers,
//                 &mut cnt,
//             )?;
//         }
//     }

//     textures.push(white_texture);
//     samplers.push(sampler);

//     Ok(Scene {
//         textures,
//         objects,
//         buffers,
//         samplers,
//     })
// }

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

        Ok(App {
            window,
            world_renderer,
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
            viewproj: view * proj,
        };

        let scene = Scene {
            camera,
            draw_items: &[],
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
