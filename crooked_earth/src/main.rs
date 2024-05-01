//! A simple 3D scene with light shining over a cube sitting on a plane.

mod earth_camera;
mod earth_fetch;
mod geo_trig;
mod util;

use bevy::prelude::*;
mod bevy_tokio_tasks;
use bevy_tokio_tasks::TokioTasksPlugin;

fn main() {
    App::new()
        // .add_plugins(bevy_web_asset::WebAssetPlugin)
        .add_plugins(DefaultPlugins)
        .add_plugins(TokioTasksPlugin::default())
        // .add_plugins(bevy_trackball::TrackballPlugin)
        .add_plugins(earth_camera::EarthCameraPlugin {})
        .add_plugins(earth_fetch::EarthFetchPlugin {})
        .add_systems(Startup, setup_lights)
        .run();
}

fn setup_lights(mut commands: Commands) {
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });
}
