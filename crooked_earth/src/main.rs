use bevy::prelude::*;

use crooked_earth::bevy_tokio_tasks::TokioTasksPlugin;

use bevy::{
    pbr::wireframe::{NoWireframe, Wireframe, WireframeColor, WireframeConfig, WireframePlugin},
    prelude::*,
    render::{
        render_resource::WgpuFeatures,
        settings::{RenderCreation, WgpuSettings},
        RenderPlugin,
    },
};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Crooked Earth.".into(),
                    name: Some("Crooked.Earth".into()),
                    resolution: (1920., 1080.).into(),
                    present_mode: bevy::window::PresentMode::AutoNoVsync,
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
            }).set(RenderPlugin {
                render_creation: RenderCreation::Automatic(WgpuSettings {
                    // WARN this is a native only feature. It will not work with webgl or webgpu
                    features: WgpuFeatures::POLYGON_MODE_LINE,
                    ..default()
                }),
                ..default()
            })
            .build()
            .disable::<TransformPlugin>(),))
        .add_plugins((
            big_space::FloatingOriginPlugin::<i64>::default(),
            // big_space::debug::FloatingOriginDebugPlugin::<i64>::default(),
            // big_space::camera::CameraControllerPlugin::<i64>::default(),
            // bevy_framepace::FramepacePlugin,
        ))
        .add_plugins((
            bevy_egui::EguiPlugin,
            TokioTasksPlugin::default(),
            bevy_inspector_egui::quick::WorldInspectorPlugin::new(),
            // bevy::pbr::wireframe::WireframePlugin
        ))
        .insert_resource(WireframeConfig {
            // The global wireframe config enables drawing of wireframes on every mesh,
            // except those with `NoWireframe`. Meshes with `Wireframe` will always have a wireframe,
            // regardless of the global configuration.
            global: true,
            // Controls the default color of all wireframes. Used as the default color for global wireframes.
            // Can be changed per mesh using the `WireframeColor` component.
            default_color: Color::WHITE,
        }).add_plugins((
            crooked_earth::earth_fetch::EarthFetchPlugin {},
            crooked_earth::spawn_universe::SpawnUniversePlugin {},
            crooked_earth::input_events::InputEventsPlugin {},
            crooked_earth::earth_camera::EarthCameraPlugin {},
            crooked_earth::diagnostics::CustomDiagnosticsPlugin {},
            crooked_earth::config_tileserver::ConfigTileServersPlugin {},
        ))
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 100.0,
        })
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

fn ignore_all_ui_nodes_from_floating_origin(
    mut commands: Commands,
    q: Query<(Entity, Option<&big_space::IgnoreFloatingOrigin>), With<Node>>,
) {
    for (node_ent, node_ignored) in q.iter() {
        if node_ignored.is_none() {
            commands
                .entity(node_ent)
                .insert(big_space::IgnoreFloatingOrigin);
            info!("adding ignore to UI node #{:?}", node_ent);
        }
    }
}

fn ignore_all_non_grid_from_floating_origin(
    mut commands: Commands,
    q: Query<(
        Entity,
        Option<&big_space::IgnoreFloatingOrigin>,
        Option<&big_space::GridCell<i64>>,
        Option<&Name>,
    )>,
    frames: Res<bevy::core::FrameCount>,
) {
    if frames.0 > 10 {
        return;
    }
    for (node_ent, node_ignored, node_gridcell, node_name) in q.iter() {
        if node_ignored.is_none() && node_gridcell.is_none() {
            commands
                .entity(node_ent)
                .insert(big_space::IgnoreFloatingOrigin);
            info!(
                "adding ignore to thing without gridCell #{:?} {:?}",
                node_ent, node_name
            );
        }
    }
}
