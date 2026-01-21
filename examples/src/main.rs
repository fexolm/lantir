mod camera;

use avian3d::prelude::*;
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_time::Time;
use bevy_transform::components::Transform;
use bevy_winit::WinitSettings;
use camera::ChaseCameraPlugin;
use lantir_bevy::LantirDefaultPlugins;
use lantir_render::world_renderer::WorldRenderer;

#[derive(Resource)]
struct SceneLoaded;

fn load_gltf(
    commands: &mut Commands,
    world_renderer: &WorldRenderer,
    gltf_bytes: &[u8],
) -> anyhow::Result<Vec<Entity>> {
    let scene = lantir_gltf::load_gltf(world_renderer, gltf_bytes)?;

    Ok(scene.spawn(commands))
}

#[derive(Component)]
struct Car;

#[derive(Component)]
struct Ground;

fn spawn_physics_ground(mut commands: Commands) {
    // A big flat collider so the car has something to drive on.
    // We'll replace this with proper scene/mesh colliders later.
    commands.spawn((
        Ground,
        Transform::from_xyz(0.0, -0.5, 0.0),
        RigidBody::Static,
        Collider::cuboid(2000.0, 0.5, 2000.0),
        Friction::new(1.0),
        Restitution::new(0.0),
    ));
}

fn car_drive_system(
    keyboard_input: Res<bevy_input::ButtonInput<bevy_input::keyboard::KeyCode>>,
    time: Res<Time>,
    mut query: Query<(&Transform, &mut LinearVelocity, &mut AngularVelocity), With<Car>>,
) {
    let dt = time.delta_secs().min(0.05);

    let throttle = (keyboard_input.pressed(bevy_input::keyboard::KeyCode::KeyW) as i8
        - keyboard_input.pressed(bevy_input::keyboard::KeyCode::KeyS) as i8)
        as f32;
    let steer = (keyboard_input.pressed(bevy_input::keyboard::KeyCode::KeyD) as i8
        - keyboard_input.pressed(bevy_input::keyboard::KeyCode::KeyA) as i8) as f32;

    // Tuned driving parameters
    let max_speed = 20.0; // reduced top speed
    let accel = 50.0; // reduced acceleration
    let turn_rate = 2.0; // gentler steering
    let lateral_damping = 6.0;

    for (transform, mut linear_velocity, mut angular_velocity) in query.iter_mut() {
        let forward = transform
            .rotation
            .mul_vec3(glam::Vec3::new(0.0, 0.0, 1.0))
            .normalize_or_zero();
        let right = forward.cross(glam::Vec3::Y).normalize_or_zero();

        // Keep existing vertical motion from physics.
        let v = linear_velocity.0;
        let horizontal = glam::Vec3::new(v.x, 0.0, v.z);

        let mut forward_speed = horizontal.dot(forward);
        let mut right_speed = horizontal.dot(right);

        // Drive towards a target speed along the forward axis.
        let desired_forward_speed = throttle * max_speed;
        let max_forward_delta = accel * dt;
        forward_speed +=
            (desired_forward_speed - forward_speed).clamp(-max_forward_delta, max_forward_delta);

        // Arcade-style sideways damping to keep the car controllable.
        right_speed -= right_speed * lateral_damping * dt;

        let new_horizontal = forward * forward_speed + right * right_speed;
        linear_velocity.0 = glam::Vec3::new(new_horizontal.x, v.y, new_horizontal.z);

        // Turn rate scales slightly with speed so steering isn't too twitchy at rest.
        // Invert steering sign to match expected input (D = turn right).
        // Reverse steering when the car is moving backward so controls feel natural.
        let speed_factor = (forward_speed.abs() / max_speed).clamp(0.2, 1.0);
        let motion_dir = if forward_speed.abs() > 0.1 {
            forward_speed.signum()
        } else {
            throttle.signum()
        };
        angular_velocity.0 =
            glam::Vec3::new(0.0, -steer * motion_dir * turn_rate * speed_factor, 0.0);
    }
}

fn load_scene_system(world_renderer: Res<WorldRenderer>, mut commands: Commands) {
    load_gltf(
        &mut commands,
        &*world_renderer,
        include_bytes!("../assets/drift_track.glb"),
    )
    .expect("load_track");

    let car = load_gltf(
        &mut commands,
        &*world_renderer,
        include_bytes!("../assets/porsche.glb"),
    )
    .expect("load_porsche")[0];
    commands.entity(car).insert(Car);

    world_renderer
        .resource_manager()
        .set_skybox_image(
            image::load_from_memory(include_bytes!("../assets/sunset.exr"))
                .expect("failed to load skybox image"),
            1.0,
            0.35,
        )
        .expect("failed to set skybox image");
    commands.insert_resource(SceneLoaded);
}

fn setup_car_physics_system(
    mut commands: Commands,
    scene_loaded: Option<Res<SceneLoaded>>,
    mut query: Query<(Entity, &mut Transform), With<Car>>,
) {
    // Run only once when the scene is loaded.
    let Some(_flag) = scene_loaded else { return };

    for (entity, mut transform) in query.iter_mut() {
        // Local forward is +Z in this coordinate convention.
        let forward = transform.rotation.mul_vec3(glam::Vec3::new(0.0, 0.0, 1.0));
        transform.translation += forward * 6.0;
        transform.translation.y += 2.0;

        commands.entity(entity).insert((
            RigidBody::Dynamic,
            // Very rough placeholder collider. We'll replace with a better approximation later.
            Collider::cuboid(1.0, 0.6, 2.2),
            // Keep the car upright (no roll/pitch) while still allowing yaw.
            LockedAxes::new().lock_rotation_x().lock_rotation_z(),
            LinearVelocity::ZERO,
            AngularVelocity::ZERO,
            LinearDamping(0.4),
            AngularDamping(2.5),
            Friction::new(1.0),
            Restitution::new(0.0),
        ));
    }

    commands.remove_resource::<SceneLoaded>();
}

fn main() -> anyhow::Result<()> {
    let mut app = App::new();

    app.add_plugins(LantirDefaultPlugins {
        title: "Lantir Example".to_string(),
    });
    app.insert_resource(WinitSettings::game());

    app.add_plugins(PhysicsPlugins::default());
    app.add_systems(bevy_app::Startup, spawn_physics_ground);
    app.add_systems(
        PhysicsSchedule,
        car_drive_system
            .after(PhysicsStepSystems::First)
            .before(PhysicsStepSystems::BroadPhase),
    );

    app.add_plugins(ChaseCameraPlugin);

    app.add_systems(Update, load_scene_system.run_if(run_once));
    app.add_systems(Update, setup_car_physics_system);

    app.run();
    Ok(())
}
