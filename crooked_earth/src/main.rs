//! A simple 3D scene with light shining over a cube sitting on a plane.

mod earth_camera;
mod earth_fetch;
mod geo_trig;
mod input_events;
mod util;

use bevy::prelude::*;
mod bevy_tokio_tasks;
use bevy_tokio_tasks::TokioTasksPlugin;

use bevy::{
    core::FrameCount,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    window::{CursorGrabMode, PresentMode, WindowLevel, WindowTheme},
};

use bevy_screen_diagnostics::{ScreenDiagnosticsPlugin, ScreenFrameDiagnosticsPlugin, ScreenEntityDiagnosticsPlugin};

fn main() {
    App::new()
        // .add_plugins(bevy_web_asset::WebAssetPlugin)
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Crooked Earth.".into(),
                    name: Some("Crooked.Earth".into()),
                    resolution: (1920., 1080.).into(),
                    present_mode: PresentMode::AutoVsync,
                    // Tells wasm not to override default event handling, like F5, Ctrl+R etc.
                    prevent_default_event_handling: false,
                    window_theme: Some(WindowTheme::Dark),
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
            }),
            // LogDiagnosticsPlugin::default(),
            // FrameTimeDiagnosticsPlugin,
        ))
        // DIAGNOSTICS
        .add_plugins(ScreenDiagnosticsPlugin::default())
        .add_plugins(ScreenFrameDiagnosticsPlugin)
        .add_plugins(ScreenEntityDiagnosticsPlugin)

        // .add_plugins(DiagnosticVisualizerEguiPlugin::default())
        .add_plugins(TokioTasksPlugin::default())
        .add_plugins(bevy_trackball::TrackballPlugin)
        .add_plugins(bevy_egui::EguiPlugin)

        .add_plugins(earth_camera::EarthCameraPlugin {})
        .add_plugins(earth_fetch::EarthFetchPlugin {})
        .add_plugins(input_events::InputEventsPlugin {})
        
        .add_systems(Update, make_window_visiblea_after_3_frames)
        .run();
}

fn make_window_visiblea_after_3_frames(
    mut window: Query<&mut Window>,
    frames: Res<FrameCount>,
) {
    if frames.0 == 3 {
        window.single_mut().visible = true;
    }
}
