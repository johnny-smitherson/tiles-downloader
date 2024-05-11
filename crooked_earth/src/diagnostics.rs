use bevy::{
    diagnostic::{Diagnostic, DiagnosticPath, Diagnostics, RegisterDiagnostic},
    prelude::*,
};

use bevy_screen_diagnostics::{
    Aggregate, ScreenDiagnostics, ScreenDiagnosticsPlugin,
    ScreenEntityDiagnosticsPlugin, ScreenFrameDiagnosticsPlugin,
};

use crate::earth_fetch::*;

pub struct CustomDiagnosticsPlugin {}

const DL_PENDING: DiagnosticPath = DiagnosticPath::const_new("dl_pending");
const DL_STARTED: DiagnosticPath = DiagnosticPath::const_new("dl_started");
const DL_FINISHED: DiagnosticPath = DiagnosticPath::const_new("dl_finished");
const ASSET_IMAGE_COUNT: DiagnosticPath =
    DiagnosticPath::const_new("image_count");
const ASSET_STDMAT_COUNT: DiagnosticPath =
    DiagnosticPath::const_new("mat_count");
const ASSET_MESH_COUNT: DiagnosticPath =
    DiagnosticPath::const_new("mesh_count");

impl Plugin for CustomDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ScreenDiagnosticsPlugin::default(),
            ScreenFrameDiagnosticsPlugin,
            ScreenEntityDiagnosticsPlugin,
        ))
        .register_diagnostic(Diagnostic::new(DL_PENDING))
        .register_diagnostic(Diagnostic::new(DL_STARTED))
        .register_diagnostic(Diagnostic::new(DL_FINISHED))
        .register_diagnostic(Diagnostic::new(ASSET_IMAGE_COUNT))
        .register_diagnostic(Diagnostic::new(ASSET_STDMAT_COUNT))
        .register_diagnostic(Diagnostic::new(ASSET_MESH_COUNT))
        .add_systems(Startup, setup_diagnostic)
        .add_systems(Update, count_diagnostic);
    }
}

fn setup_diagnostic(mut onscreen: ResMut<ScreenDiagnostics>) {
    onscreen
        .add("dl/pending;".to_string(), DL_PENDING)
        .aggregate(Aggregate::Value)
        .format(|v| format!("{v:.0}"));

    onscreen
        .add("dl/started;".to_string(), DL_STARTED)
        .aggregate(Aggregate::Value)
        .format(|v| format!("{v:.0}"));

    onscreen
        .add("dl/done;".to_string(), DL_FINISHED)
        .aggregate(Aggregate::Value)
        .format(|v| format!("{v:.0}"));

    onscreen
        .add("asset/images;".to_string(), ASSET_IMAGE_COUNT)
        .aggregate(Aggregate::Value)
        .format(|v| format!("{v:.0}"));

    onscreen
        .add("asset/material;".to_string(), ASSET_STDMAT_COUNT)
        .aggregate(Aggregate::Value)
        .format(|v| format!("{v:.0}"));

    onscreen
        .add("asset/meshes;".to_string(), ASSET_MESH_COUNT)
        .aggregate(Aggregate::Value)
        .format(|v| format!("{v:.0}"));
}

fn count_diagnostic(
    mut diagnostics: Diagnostics,
    q_pending: Query<&DownloadPending>,
    q_started: Query<&DownloadStarted>,
    q_finished: Query<&DownloadFinished>,
    asset_meshes: Res<Assets<Mesh>>,
    asset_stdmat: Res<Assets<StandardMaterial>>,
    asset_img: Res<Assets<Image>>,
    // q_img: Query<&Handle<Image>>,
) {
    diagnostics.add_measurement(&DL_PENDING, || q_pending.iter().len() as f64);

    diagnostics.add_measurement(&DL_STARTED, || q_started.iter().len() as f64);

    diagnostics
        .add_measurement(&DL_FINISHED, || q_finished.iter().len() as f64);

    diagnostics.add_measurement(&ASSET_MESH_COUNT, || {
        asset_meshes.iter().count() as f64
    });

    diagnostics.add_measurement(&ASSET_STDMAT_COUNT, || {
        asset_stdmat.iter().count() as f64
    });

    diagnostics.add_measurement(&ASSET_IMAGE_COUNT, || {
        asset_img.iter().count() as f64
    });
}
