mod camera;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_input::keyboard::KeyboardInput;
use bevy_input::mouse::{MouseButton, MouseButtonInput, MouseWheel};
use bevy_input::ButtonState;
use bevy_window::{CursorMoved, PrimaryWindow, Window, WindowPlugin, WindowResized};
use bevy_winit::{WinitPlugin, WinitWindows};
use lantir_render::resources::{DrawItem, INVALID_RESOURCE_HANDLE};
use lantir_render::scene::{Camera, Scene};
use lantir_render::world_renderer::{self, WorldRenderer, WorldRendererConfig};
use lantir_hal::{RenderEngine, RenderEngineConfig, vk};
use std::time::Instant;

use crate::camera::{CameraInput, CameraState, OrbitCamera};

const WINDOW_WIDTH: u32 = 1300;
const WINDOW_HEIGHT: u32 = 900;

#[derive(Resource)]
struct FrameTime {
    last_frame_time: Instant,
    dt: f32,
}

impl Default for FrameTime {
    fn default() -> Self {
        Self {
            last_frame_time: Instant::now(),
            dt: 0.0,
        }
    }
}

#[derive(Resource)]
struct DrawItems {
    draw_items: Vec<DrawItem>,
}

struct RenderState {
    world_renderer: WorldRenderer,
}

fn init_render_exclusive(world: &mut World) {
    if world.get_non_send_resource::<RenderState>().is_some() {
        return;
    }

    // The window is created by bevy_window/bevy_winit.
    let primary_entity = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .iter(world)
        .next()
        .expect("PrimaryWindow entity missing");

    // WinitWindows is stored as a non-send resource.
    let winit_windows = world
        .get_non_send_resource::<WinitWindows>()
        .expect("WinitWindows missing (did you add WinitPlugin?)");
    let Some(winit_window) = winit_windows.get_window(primary_entity) else {
        // Not ready yet; winit window is created very early, but this keeps init robust.
        return;
    };

    let image_extent = vk::Extent2D {
        width: winit_window.inner_size().width,
        height: winit_window.inner_size().height,
    };
    let draw_extent = image_extent;

    let engine = {
        let config = RenderEngineConfig {
            debug: true,
            frames_in_flight: 2,
        };
        RenderEngine::new(&**winit_window, &config).expect("RenderEngine::new")
    };

    let world_renderer = world_renderer::WorldRenderer::new(
        engine,
        &WorldRendererConfig {
            draw_extent,
            window_extent: draw_extent,
        },
    )
    .expect("WorldRenderer::new");

    let draw_items = load_gltf_draw_items(&world_renderer, include_bytes!("../assets/track.glb"))
        .expect("load_gltf_draw_items");
    world.insert_resource(DrawItems { draw_items });

    // Initialize camera state once we know the draw extent.
    let orbit = *world
        .get_resource::<OrbitCamera>()
        .expect("OrbitCamera missing");
    let initial_view = orbit.view_matrix();
    let initial_camera = make_camera(initial_view, world_renderer.draw_extent());
    world.insert_resource(CameraState {
        camera: initial_camera,
    });

    world.insert_non_send_resource(RenderState { world_renderer });
}

fn make_camera(view: glam::Mat4, draw_extent: vk::Extent2D) -> Camera {
    let mut proj: glam::Mat4 = glam::Mat4::perspective_rh(
        70f32.to_radians(),
        draw_extent.width as f32 / draw_extent.height as f32,
        0.1,
        10000.0,
    );
    proj.y_axis.y *= -1.0;

    Camera {
        view,
        proj,
        viewproj: proj * view,
    }
}

fn load_gltf_draw_items(world_renderer: &WorldRenderer, gltf_bytes: &[u8]) -> anyhow::Result<Vec<DrawItem>> {
    let scene = lantir_gltf::load_gltf(world_renderer, gltf_bytes)?;
    Ok(scene
        .nodes
        .into_iter()
        .filter_map(|node| {
            let mesh = node.mesh?;
            let material = node.material.unwrap_or(INVALID_RESOURCE_HANDLE);
            Some(DrawItem {
                transform: node.transform,
                mesh,
                material,
            })
        })
        .collect())
}

fn update_time_system(mut time: ResMut<FrameTime>) {
    let now = Instant::now();
    time.dt = (now - time.last_frame_time).as_secs_f32().min(0.1);
    time.last_frame_time = now;
}

fn update_camera_system(mut orbit: ResMut<OrbitCamera>, mut input: ResMut<CameraInput>, time: Res<FrameTime>) {
    orbit.update(&mut *input, time.dt);
}

fn input_system(
    mut input: ResMut<CameraInput>,
    mut keyboard_events: EventReader<KeyboardInput>,
    mut mouse_button_events: EventReader<MouseButtonInput>,
    mut cursor_moved_events: EventReader<CursorMoved>,
    mut mouse_wheel_events: EventReader<MouseWheel>,
) {
    for ev in keyboard_events.read() {
        input.set_key(ev.key_code, ev.state == ButtonState::Pressed);
    }

    for ev in mouse_button_events.read() {
        if ev.button == MouseButton::Left {
            input.mouse_pressed = ev.state == ButtonState::Pressed;
            if !input.mouse_pressed {
                input.last_cursor_pos = None;
            }
        }
    }

    for ev in cursor_moved_events.read() {
        if input.mouse_pressed {
            let x = ev.position.x;
            let y = ev.position.y;
            if let Some((lx, ly)) = input.last_cursor_pos {
                input.add_mouse_delta(x - lx, y - ly);
            }
            input.last_cursor_pos = Some((x, y));
        }
    }

    for ev in mouse_wheel_events.read() {
        // In Bevy, positive y usually means scroll up.
        input.add_scroll(ev.y);
    }
}

fn resize_system(mut render: NonSendMut<RenderState>, mut events: EventReader<WindowResized>) {
    for ev in events.read() {
        render
            .world_renderer
            .resize(vk::Extent2D {
                width: ev.width as u32,
                height: ev.height as u32,
            })
            .unwrap();
    }
}

fn build_camera_state_system(
    orbit: Res<OrbitCamera>,
    mut state: ResMut<CameraState>,
    render: NonSend<RenderState>,
) {
    let view = orbit.view_matrix();

    let draw_extent = render.world_renderer.draw_extent();
    state.camera = make_camera(view, draw_extent);
}

fn render_system(
    mut render: NonSendMut<RenderState>,
    draw_items: Res<DrawItems>,
    camera: Res<CameraState>,
) {
    let scene = Scene {
        camera: camera.camera,
        draw_items: &draw_items.draw_items,
    };

    render.world_renderer.draw_frame(&scene).unwrap();
}

fn render_ready(render: Option<NonSend<RenderState>>) -> bool {
    render.is_some()
}

fn main() -> anyhow::Result<()> {
    let mut app = App::new();

    // Use Bevy's winit integration (event loop + window creation).
    app.add_plugins(WindowPlugin {
        primary_window: Some(Window {
            title: "Example App".to_string(),
            resolution: (WINDOW_WIDTH as f32, WINDOW_HEIGHT as f32).into(),
            ..Default::default()
        }),
        ..Default::default()
    });
    app.add_plugins(bevy_a11y::AccessibilityPlugin::default());
    app.add_plugins(WinitPlugin::<bevy_winit::WakeUp>::default());

    // ECS state
    app.insert_resource(CameraInput::default());
    app.insert_resource(OrbitCamera::default());
    app.insert_resource(FrameTime::default());

    // Init lantir renderer once the winit window exists.
    app.add_systems(Update, init_render_exclusive);

    // Always-on systems
    app.add_systems(
        Update,
        (input_system, update_time_system, update_camera_system).chain(),
    );

    // Render-only systems (need RenderState)
    app.add_systems(
        Update,
        (resize_system, build_camera_state_system, render_system)
            .chain()
            .run_if(render_ready),
    );

    app.run();
    Ok(())
}
