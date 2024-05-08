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
    window::{PresentMode, WindowTheme},
};

use bevy_screen_diagnostics::{
    ScreenDiagnosticsPlugin, ScreenEntityDiagnosticsPlugin,
    ScreenFrameDiagnosticsPlugin,
};
use big_space::IgnoreFloatingOrigin;

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
        .add_plugins(input_events::InputEventsPlugin {})
        .add_plugins(earth_fetch::EarthFetchPlugin {})
        .add_systems(Update, make_window_visible_after_3_frames)
        .add_systems(Update, ignore_all_ui_nodes_from_floating_origin)
        .add_systems(Update, egui_example_system)
        .run();
}

fn make_window_visible_after_3_frames(
    mut window: Query<&mut Window>,
    frames: Res<FrameCount>,
    mut commands: Commands,
) {
    if frames.0 == 3 {
        window.single_mut().visible = true;
        commands.spawn(SpatialBundle::default()).insert(
            earth_fetch::WebMercatorTiledPlanet {
                root_zoom_level: 4,
            },
        );
    }
}

fn ignore_all_ui_nodes_from_floating_origin(mut commands: Commands, q: Query<(Entity, Option<&IgnoreFloatingOrigin>), With<Node>>) {
    for (node_ent, node_ignored) in q.iter() {
        if node_ignored.is_none() {
            commands.entity(node_ent).insert(IgnoreFloatingOrigin);
            info!("adding ignore to UI node #{:?}", node_ent);
        }
    }
}


fn egui_example_system(mut contexts: bevy_egui::EguiContexts) {
    bevy_egui::egui::Window::new("Hello").show(contexts.ctx_mut(), |ui| {
        ui.label("world");
    });
}