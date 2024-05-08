use bevy::prelude::*;
/// Example with spheres at the scale and distance of the earth and moon around the sun, at 1:1
/// scale. The earth is rotating on its axis, and the camera is in this reference frame, to
/// demonstrate how high precision nested reference frames work at large scales.
use bevy::{core_pipeline::bloom::BloomSettings, render::camera::Exposure};

use big_space::FloatingSpatialBundle;
use big_space::{
    // camera::CameraController,
    reference_frame::{ReferenceFrame, RootReferenceFrame},
    FloatingOrigin,
    GridCell,
};
use rand::Rng;

use crate::earth_camera::EarthCamera;
use crate::earth_fetch::WebMercatorTiledPlanet;

pub struct SpawnUniversePlugin {}

impl Plugin for SpawnUniversePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_universe)
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, spawn_stars)
            .add_systems(Update, rotate)
            .add_systems(Update, spawn_sun)
            .add_systems(Update, spawn_planet)
            .add_systems(Update, spawn_moon)
            .add_systems(Update, reparent_camera)
            .add_systems(Update, spawn_the_ball);
    }
}

#[derive(Component, Debug, Reflect)]
struct TheUniverse;

#[derive(Component, Debug, Reflect)]
struct TheSun;

#[derive(Component, Debug, Reflect)]
struct TheSunMesh;

#[derive(Component, Debug, Reflect)]
struct SomeStar;

#[derive(Component, Debug, Reflect)]
struct ThePlanet;

#[derive(Component, Debug, Reflect)]
struct TheMoon;

#[derive(Component, Debug, Reflect)]
struct TheBall;

#[derive(Component, Debug, Reflect)]
struct Rotates(f32);

fn rotate(mut rotate_query: Query<(&mut Transform, &Rotates)>) {
    for (mut transform, rotates) in rotate_query.iter_mut() {
        transform.rotate_local_y(rotates.0);
    }
}

fn spawn_universe(mut commands: Commands) {
    info!("spawn universe");
    commands.spawn((
        FloatingSpatialBundle::<i64>::default(),
        TheUniverse,
        Name::new("The Universe"),
        ReferenceFrame::<i64>::default(),
    ));
}

fn spawn_camera(mut commands: Commands) {
    info!("setup_camera");
    commands.spawn((
        Name::new("main 3D camera"),
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
                        // CameraController::default() // Built-in camera controller
                        //     .with_speed_bounds([10e-18, 10e35])
                        //     .with_smoothness(0.9, 0.8)
                        //     .with_speed(1.0),
    ));
}

fn spawn_stars(
    parent: Query<Entity, Added<TheUniverse>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let parent = if parent.get_single().is_ok() {
        parent.single()
    } else {
        return;
    };
    info!("spawn_stars");
    let parent = commands
        .spawn((
            Name::new("Sky Stars"),
            FloatingSpatialBundle::<i64>::default(),
            ReferenceFrame::<i64>::default(),
        ))
        .set_parent(parent)
        .id();
    let mut sphere =
        |radius| meshes.add(Sphere::new(radius).mesh().ico(4).unwrap());

    let star = sphere(1e10);
    let star_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: Color::rgb_linear(100000., 100000., 100000.),
        ..default()
    });
    let mut rng = rand::thread_rng();
    for i in 0..500 {
        commands
            .spawn((
                Name::new(format!("star #{}", i)),
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
            ))
            .set_parent(parent);
    }
}

fn spawn_sun(
    parent: Query<Entity, Added<TheUniverse>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let parent = if parent.get_single().is_ok() {
        parent.single()
    } else {
        return;
    };

    info!("spawn_sun");
    let mut sphere =
        |radius| meshes.add(Sphere::new(radius).mesh().ico(4).unwrap());

    let sun_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: Color::rgb_linear(10000000., 10000000., 10000000.),
        ..default()
    });

    commands
        .spawn((
            Name::new("The Sun"),
            GridCell::<i64>::ZERO,
            PointLightBundle {
                point_light: PointLight {
                    intensity: 35.73e27,
                    range: 1e20,
                    radius: crate::universal_const::SUN_RADIUS_M,
                    shadows_enabled: true,
                    ..default()
                },
                ..default()
            },
            TheSun,
            ReferenceFrame::<i64>::default(),
        ))
        .set_parent(parent)
        .with_children(|builder| {
            builder.spawn((
                Name::new("The Sun Mesh"),
                PbrBundle {
                    mesh: sphere(crate::universal_const::SUN_RADIUS_M),
                    material: sun_mat,
                    ..default()
                },
                TheSunMesh,
                GridCell::<i64>::ZERO,
            ));
        });
}

fn spawn_planet(
    parent: Query<Entity, Added<TheUniverse>>,
    mut commands: Commands,
    space: Res<RootReferenceFrame<i64>>,
) {
    let parent = if parent.get_single().is_ok() {
        parent.single()
    } else {
        return;
    };
    info!("when_the_sun_appears_spawn_the_planet");

    let earth_orbit_radius_m = 149.60e9;

    let (earth_cell, earth_pos): (GridCell<i64>, _) =
        space.imprecise_translation_to_grid(Vec3::Z * earth_orbit_radius_m);

    commands
        .spawn((
            // PbrBundle {
            //     mesh: sphere(EARTH_RADIUS_M),
            //     material: earth_mat,
            //     transform: ,
            //     ..default()
            // },
            Name::new("The Planet"),
            FloatingSpatialBundle {
                grid_position: earth_cell,
                transform: Transform::from_translation(earth_pos),
                ..default()
            },
            ReferenceFrame::<i64>::default(),
            Rotates(0.001),
            ThePlanet,
            WebMercatorTiledPlanet {
                planet_name: "earth".into(),
                root_zoom_level: 5,
                tile_type: "arcgis_sat".into(),
                planet_radius: crate::universal_const::EARTH_RADIUS_M as f64,
            },
        ))
        .set_parent(parent);
}

fn spawn_moon(
    parent: Query<Entity, Added<ThePlanet>>,
    mut commands: Commands,
    space: Res<RootReferenceFrame<i64>>,
) {
    if !parent.get_single().is_ok() {
        return;
    }
    let parent = parent.single();
    info!("when_the_planet_appears_spawn_the_moon");

    let (moon_cell, moon_pos): (GridCell<i64>, _) = space
        .imprecise_translation_to_grid(
            Vec3::X * crate::universal_const::MOON_ORBIT_RADIUS_M,
        );

    commands
        .spawn((
            Name::new("Moon Orbit Reference"),
            SpatialBundle::default(),
            GridCell::<i64>::ONE,
            ReferenceFrame::<i64>::default(),
            Rotates(0.01),
        ))
        .set_parent(parent)
        .with_children(|commands| {
            commands.spawn((
                Name::new("The Moon"),
                FloatingSpatialBundle {
                    grid_position: moon_cell,
                    transform: Transform::from_translation(moon_pos),
                    ..default()
                },
                TheMoon,
                WebMercatorTiledPlanet {
                    planet_name: "moon".into(),
                    root_zoom_level: 4,
                    tile_type: "google_moon".into(),
                    planet_radius: crate::universal_const::MOON_RADIUS_M as f64,
                },
                ReferenceFrame::<i64>::default(),
                Rotates(0.05),
            ));
        });
}

fn reparent_camera(
    parent: Query<Entity, Added<ThePlanet>>,
    mut commands: Commands,
    // space: Res<RootReferenceFrame<i64>>,
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

    let (camera_ent, mut _camera_gridcell_ref, mut camera_trans_ref) =
        camera_q.single_mut();

    let cam_info = EarthCamera::from_planet_radius(
        crate::universal_const::EARTH_RADIUS_M as f64,
    );
    // let (cam_cell, cam_pos): (GridCell<i64>, _) =
    //     space.imprecise_translation_to_grid(cam_info.get_transform().);

    commands
        .entity(camera_ent)
        .set_parent(parent)
        .insert(cam_info.clone());
    // *camera_gridcell_ref = cam_cell;
    *camera_trans_ref = cam_info.get_abs_transform();
}

fn spawn_the_ball(
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
            Vec3::X * (crate::universal_const::EARTH_RADIUS_M + 1.0)
                + Vec3::NEG_Z * 5.0,
        );

    let ball_mat = materials.add(StandardMaterial {
        base_color: Color::FUCHSIA,
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..default()
    });

    commands
        .spawn((
            Name::new("The Ball"),
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
