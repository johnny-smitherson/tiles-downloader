use bevy::prelude::*;
/// Example with spheres at the scale and distance of the earth and moon around the sun, at 1:1
/// scale. The earth is rotating on its axis, and the camera is in this reference frame, to
/// demonstrate how high precision nested reference frames work at large scales.
use bevy::{
    core_pipeline::bloom::BloomSettings, prelude::*, render::camera::Exposure,
};
use bevy_screen_diagnostics::{
    ScreenDiagnosticsPlugin, ScreenEntityDiagnosticsPlugin,
    ScreenFrameDiagnosticsPlugin,
};
use big_space::FloatingSpatialBundle;
use big_space::{
    camera::CameraController,
    reference_frame::{ReferenceFrame, RootReferenceFrame},
    FloatingOrigin, GridCell,
};
use rand::Rng;
mod bevy_tokio_tasks;
use bevy_tokio_tasks::TokioTasksPlugin;

use crate::earth_fetch::WebMercatorTiledPlanet;
mod earth_fetch;
mod geo_trig;
mod util;

#[derive(Component)]
struct TheSun;

#[derive(Component)]
struct TheSunMesh;

#[derive(Component)]
struct SomeStar;

#[derive(Component)]
struct ThePlanet;

#[derive(Component)]
struct TheMoon;

#[derive(Component)]
struct TheBall;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Crooked Earth.".into(),
                    name: Some("Crooked.Earth".into()),
                    resolution: (1920., 1080.).into(),
                    present_mode: bevy::window::PresentMode::AutoVsync,
                    // Tells wasm not to override default event handling, like F5, Ctrl+R etc.
                    prevent_default_event_handling: false,
                    window_theme: Some(bevy::window::WindowTheme::Dark),
                    enabled_buttons: bevy::window::EnabledButtons {
                        maximize: true,
                        ..Default::default()
                    },
                    // This will spawn an invisible window
                    // The window will be made visible in the make_visible() system after 3 frames.
                    // This is useful when you want to avoid the white window that shows up before the GPU is ready to render the app.
                    visible: false,
                    ..default()
                }),
                ..default()
            })
            .build()
            .disable::<TransformPlugin>(),))
        .add_plugins((
            big_space::FloatingOriginPlugin::<i64>::default(),
            // big_space::debug::FloatingOriginDebugPlugin::<i64>::default(),
            big_space::camera::CameraControllerPlugin::<i64>::default(),
            // bevy_framepace::FramepacePlugin,
        ))
        .add_plugins((
            bevy_egui::EguiPlugin,
            ScreenDiagnosticsPlugin::default(),
            ScreenFrameDiagnosticsPlugin,
            ScreenEntityDiagnosticsPlugin,
            TokioTasksPlugin::default(),
            bevy_inspector_egui::quick::WorldInspectorPlugin::new(),
        ))

        .add_plugins((crate::earth_fetch::EarthFetchPlugin {},))
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 100.0,
        })
        .add_systems(Startup, setup_sun)
        .add_systems(Startup, setup_camera)
        .add_systems(Update, rotate)
        .add_systems(Update, when_the_sun_appears_spawn_the_planet)
        .add_systems(Update, when_the_planet_appears_spawn_the_moon)
        .add_systems(Update, when_the_planet_appears_reset_the_camera)
        .add_systems(Update, when_the_planet_appears_spawn_the_ball)
        .add_systems(Update, make_window_visible_after_3_frames)
        .add_systems(Update, ignore_all_ui_nodes_from_floating_origin)
        .add_systems(Update, ignore_all_non_grid_from_floating_origin)
        
        .run()
}
fn make_window_visible_after_3_frames(
    mut window: Query<&mut Window>,
    frames: Res<bevy::core::FrameCount>,
) {
    if frames.0 == 3 {
        window.single_mut().visible = true;
        info!("make window visible.");
    }
}


fn ignore_all_ui_nodes_from_floating_origin(mut commands: Commands, q: Query<(Entity, Option<&big_space::IgnoreFloatingOrigin>), With<Node>>) {
    for (node_ent, node_ignored) in q.iter() {
        if node_ignored.is_none() {
            commands.entity(node_ent).insert(big_space::IgnoreFloatingOrigin);
            info!("adding ignore to UI node #{:?}", node_ent);
        }
    }
}


fn ignore_all_non_grid_from_floating_origin(mut commands: Commands, q: Query<(Entity, Option<&big_space::IgnoreFloatingOrigin>, Option<&big_space::GridCell<i64>>)>) {
    for (node_ent, node_ignored, node_gridcell) in q.iter() {
        if node_ignored.is_none() && node_gridcell.is_none() {
            commands.entity(node_ent).insert(big_space::IgnoreFloatingOrigin);
        }
    }
}

#[derive(Component)]
struct Rotates(f32);

const EARTH_RADIUS_M: f32 = 6.371e6;
const moon_orbit_radius_m: f32 = 3e7;
const moon_radius_m: f32 = 1.7375e6;
const sun_radius_m: f32 = 695_508_000.0;

fn rotate(mut rotate_query: Query<(&mut Transform, &Rotates)>) {
    for (mut transform, rotates) in rotate_query.iter_mut() {
        transform.rotate_local_y(rotates.0);
    }
}

fn setup_camera(mut commands: Commands) {
    info!("setup_camera");
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_translation(Vec3::ZERO)
                .looking_to(Vec3::NEG_Z, Vec3::Y),
            camera: Camera {
                hdr: true,
                ..default()
            },
            exposure: Exposure::SUNLIGHT,
            ..default()
        },
        BloomSettings::default(),
        GridCell::<i64>::ZERO,
        FloatingOrigin, // Important: marks the floating origin entity for rendering.
        CameraController::default() // Built-in camera controller
            .with_speed_bounds([10e-18, 10e35])
            .with_smoothness(0.9, 0.8)
            .with_speed(1.0),
    ));
}

fn setup_sun(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    info!("setup_sun");
    let mut sphere =
        |radius| meshes.add(Sphere::new(radius).mesh().ico(4).unwrap());

    let star = sphere(1e10);
    let star_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: Color::rgb_linear(100000., 100000., 100000.),
        ..default()
    });
    let mut rng = rand::thread_rng();
    for _ in 0..500 {
        commands.spawn((
            GridCell::<i64>::new(
                ((rng.gen::<f32>() - 0.5) * 1e11) as i64,
                ((rng.gen::<f32>() - 0.5) * 1e11) as i64,
                ((rng.gen::<f32>() - 0.5) * 1e11) as i64,
            ),
            PbrBundle {
                mesh: star.clone(),
                material: star_mat.clone(),
                ..default()
            },
            SomeStar,
        ));
    }

    let sun_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: Color::rgb_linear(10000000., 10000000., 10000000.),
        ..default()
    });

    commands
        .spawn((
            GridCell::<i64>::ZERO,
            PointLightBundle {
                point_light: PointLight {
                    intensity: 35.73e27,
                    range: 1e20,
                    radius: sun_radius_m,
                    shadows_enabled: true,
                    ..default()
                },
                ..default()
            },
            TheSun,
        ))
        .with_children(|builder| {
            builder.spawn((
                PbrBundle {
                    mesh: sphere(sun_radius_m),
                    material: sun_mat,
                    ..default()
                },
                TheSunMesh,
            ));
        });
}

fn when_the_sun_appears_spawn_the_planet(
    parent: Query<Entity, Added<TheSun>>,
    mut commands: Commands,
    space: Res<RootReferenceFrame<i64>>,
) {
    let _parent = if parent.get_single().is_ok() {
        parent.single()
    } else {return};
    info!("when_the_sun_appears_spawn_the_planet");

    let earth_orbit_radius_m = 149.60e9;

    let (earth_cell, earth_pos): (GridCell<i64>, _) =
        space.imprecise_translation_to_grid(Vec3::Z * earth_orbit_radius_m);

    commands.spawn((
        // PbrBundle {
        //     mesh: sphere(EARTH_RADIUS_M),
        //     material: earth_mat,
        //     transform: ,
        //     ..default()
        // },
        FloatingSpatialBundle {
            grid_position: earth_cell,
            transform: Transform::from_translation(earth_pos),
            ..default()
        },
        ReferenceFrame::<i64>::default(),
        Rotates(0.001),
        ThePlanet,
        WebMercatorTiledPlanet {
            root_zoom_level: 5,
            tile_type: "arcgis_sat".into(),
            planet_radius: EARTH_RADIUS_M as f64,
        },
    ));
}

fn when_the_planet_appears_spawn_the_moon(
    parent: Query<Entity, Added<ThePlanet>>,
    mut commands: Commands,
    space: Res<RootReferenceFrame<i64>>,
) {
    if !parent.get_single().is_ok() {
        return;
    }
    let parent = parent.single();
    info!("when_the_planet_appears_spawn_the_moon");


    let (moon_cell, moon_pos): (GridCell<i64>, _) =
        space.imprecise_translation_to_grid(Vec3::X * moon_orbit_radius_m);

    commands
        .spawn((
            SpatialBundle::default(),
            GridCell::<i64>::ONE,
            ReferenceFrame::<i64>::default(),
            Rotates(0.009),
        ))
        .set_parent(parent)
        .with_children(|commands| {
            commands.spawn((
                FloatingSpatialBundle {
                    grid_position: moon_cell,
                    transform: Transform::from_translation(moon_pos),
                    ..default()
                },
                TheMoon,
                WebMercatorTiledPlanet {
                    root_zoom_level: 4,
                    tile_type: "google_moon".into(),
                    planet_radius: moon_radius_m as f64,
                },
                ReferenceFrame::<i64>::default(),
                Rotates(0.01),
            ));
        });
}

fn when_the_planet_appears_reset_the_camera(
    parent: Query<Entity, Added<ThePlanet>>,
    mut commands: Commands,
    space: Res<RootReferenceFrame<i64>>,
    mut camera_q: Query<
        (Entity, &mut GridCell<i64>, &mut Transform),
        With<FloatingOrigin>,
    >,
) {
    if !parent.get_single().is_ok() {
        return;
    }
    let parent = parent.single();
    info!("when_the_planet_appears_spawn_the_camera");

    let (camera_ent, mut camera_grid, mut camera_trans) = camera_q.single_mut();

    let (cam_cell, cam_pos): (GridCell<i64>, _) =
        space.imprecise_translation_to_grid(Vec3::X * (EARTH_RADIUS_M + 1.0));

    commands.entity(camera_ent).set_parent(parent);
    *camera_grid = cam_cell;
    *camera_trans =
        Transform::from_translation(cam_pos).looking_to(Vec3::NEG_Z, Vec3::X);
}

fn when_the_planet_appears_spawn_the_ball(
    parent: Query<Entity, Added<ThePlanet>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    space: Res<RootReferenceFrame<i64>>,
) {
    if !parent.get_single().is_ok() {
        return;
    }
    let parent = parent.single();
    info!("when_the_planet_appears_spawn_the_ball");

    let mut sphere =
        |radius| meshes.add(Sphere::new(radius).mesh().ico(4).unwrap());

    let (ball_cell, ball_pos): (GridCell<i64>, _) = space
        .imprecise_translation_to_grid(
            Vec3::X * (EARTH_RADIUS_M + 1.0) + Vec3::NEG_Z * 5.0,
        );

    let ball_mat = materials.add(StandardMaterial {
        base_color: Color::FUCHSIA,
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..default()
    });

    commands
        .spawn((
            PbrBundle {
                mesh: sphere(1.0),
                material: ball_mat,
                transform: Transform::from_translation(ball_pos),
                ..default()
            },
            ball_cell,
            TheBall,
        ))
        .set_parent(parent);
}
