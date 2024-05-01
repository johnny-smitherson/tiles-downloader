//! A simple 3D scene with light shining over a cube sitting on a plane.

use std::f64::consts::PI;

use bevy::prelude::*;

#[derive(Component)]
pub struct EarthCamera;

pub struct EarthCameraPlugin {}

impl Plugin for EarthCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera);
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        // bevy_trackball::TrackballController::default(),
        // bevy_trackball::TrackballCamera::look_at(target, eye, up),
        Camera3dBundle {
            transform: Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
    ));
}
