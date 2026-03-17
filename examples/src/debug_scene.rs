/// Debug binary: renders one fixed frame and saves it as a PNG.
///
/// Usage:
///   cargo run --bin debug_scene
///   LANTIR_DUMP_FRAME=debug/frames/out.png cargo run --bin debug_scene
///
/// The binary loads the Sponza atrium (sponza.glb + sunset.exr skybox),
/// positions a fixed camera inside the atrium, waits for a few frames
/// (so the renderer is fully initialized), dumps the color buffer, and exits.
use bevy_app::{App, Startup, Update};
use bevy_ecs::prelude::*;
use lantir_bevy::LantirDefaultPlugins;
use lantir_render::resources::cmgen::{
    build_prefiltered_atlas, generate_brdf_lut_image, load_cmgen_skybox_files,
};
use lantir_render::{bevy::components::Camera, scene::CameraTransform, world_renderer::WorldRenderer};

/// Marker resource: set after the scene is successfully loaded.
#[derive(Resource)]
struct SceneReady;

// ── fixed camera ─────────────────────────────────────────────────────────────

fn build_camera(eye: glam::Vec3, target: glam::Vec3) -> CameraTransform {
    let view = glam::Mat4::look_at_rh(eye, target, glam::Vec3::Y);
    let mut proj =
        glam::Mat4::perspective_infinite_reverse_rh(70f32.to_radians(), 16.0 / 9.0, 0.1);
    proj.y_axis.y *= -1.0; // flip Y for Vulkan NDC
    let viewproj = proj * view;
    CameraTransform {
        view,
        proj,
        viewproj,
        inv_viewproj: viewproj.inverse(),
        camera_pos: eye.extend(1.0),
    }
}

fn spawn_camera(mut commands: Commands) {
    // Inside the Sponza atrium, looking down the central colonnade.
    let eye    = glam::Vec3::new(0.0, 2.0, 0.0);
    let target = glam::Vec3::new(8.0, 2.0, 0.0);
    commands.spawn(Camera(build_camera(eye, target)));
}

// ── scene loading ─────────────────────────────────────────────────────────────

fn load_scene_system(
    world_renderer: Option<Res<WorldRenderer>>,
    scene_ready: Option<Res<SceneReady>>,
    mut commands: Commands,
) {
    if scene_ready.is_some() {
        return;
    }
    let Some(world_renderer) = world_renderer else {
        return;
    };

    // Load the Sponza atrium.
    lantir_gltf::load_gltf(&*world_renderer, include_bytes!("../assets/sponza.glb"))
        .expect("load sponza.glb")
        .spawn(&mut commands);

    let assets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let sky_path = assets_dir.join("sunset.exr");
    let cmgen_dir = assets_dir.join("sunset");
    let sky_image = image::open(&sky_path)
        .unwrap_or_else(|e| panic!("failed to load skybox image {}: {e}", sky_path.display()));
    let cmgen = load_cmgen_skybox_files(&cmgen_dir)
        .unwrap_or_else(|e| panic!("failed to load cmgen assets {}: {e}", cmgen_dir.display()));
    let prefiltered_image = build_prefiltered_atlas(&cmgen.prefiltered_levels)
        .expect("build prefiltered atlas");
    let brdf_lut_image = generate_brdf_lut_image(128, 128);

    world_renderer
        .resource_manager()
        .set_skybox_image(
            sky_image,
            cmgen.sh,
            prefiltered_image,
            brdf_lut_image,
            1.0,
            0.35,
        )
        .expect("set skybox");

    commands.insert_resource(SceneReady);
}

// ── frame dump + exit ─────────────────────────────────────────────────────────

#[derive(Resource)]
struct DumpState {
    path: std::path::PathBuf,
    frames_rendered: u32,
}

fn dump_and_exit_system(
    mut state: ResMut<DumpState>,
    world_renderer: Option<Res<WorldRenderer>>,
    scene_ready: Option<Res<SceneReady>>,
) {
    if scene_ready.is_none() {
        return;
    }
    let Some(renderer) = world_renderer else {
        return;
    };

    state.frames_rendered += 1;

    // Give the GPU a few frames to finish uploading assets and build the TLAS.
    if state.frames_rendered < 5 {
        return;
    }

    let path = state.path.clone();
    renderer
        .dump_frame_to_file(&path)
        .unwrap_or_else(|e| panic!("dump_frame_to_file failed: {e}"));

    eprintln!("[debug_scene] frame saved → {}", path.display());
    std::process::exit(0);
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    let dump_path = std::env::var("LANTIR_DUMP_FRAME")
        .unwrap_or_else(|_| "debug/frames/latest.png".to_string());

    let mut app = App::new();
    app.add_plugins(LantirDefaultPlugins {
        title: "Lantir Debug Scene".to_string(),
    });

    app.insert_resource(DumpState {
        path: std::path::PathBuf::from(dump_path),
        frames_rendered: 0,
    });

    app.add_systems(Startup, spawn_camera);
    app.add_systems(Update, load_scene_system);
    app.add_systems(Update, dump_and_exit_system);

    app.run();
}
