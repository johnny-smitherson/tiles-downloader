use bevy::{
    diagnostic::{Diagnostic, DiagnosticPath, Diagnostics, RegisterDiagnostic},
    prelude::*,
};

use bevy_screen_diagnostics::{
    Aggregate, ScreenDiagnostics, ScreenDiagnosticsPlugin,
    ScreenEntityDiagnosticsPlugin, ScreenFrameDiagnosticsPlugin,
};

pub struct CustomDiagnosticsPlugin {}

const DL_PENDING: DiagnosticPath = DiagnosticPath::const_new("dl_pending");
const DL_FINISHED: DiagnosticPath = DiagnosticPath::const_new("dl_finished");
// const IMAGE_COUNT: DiagnosticPath = DiagnosticPath::const_new("image_count");
const STDMAT_COUNT: DiagnosticPath = DiagnosticPath::const_new("mat_count");
const MESH_COUNT: DiagnosticPath = DiagnosticPath::const_new("mesh_count");

impl Plugin for CustomDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ScreenDiagnosticsPlugin::default(),
            ScreenFrameDiagnosticsPlugin,
            ScreenEntityDiagnosticsPlugin,
        ))
        .register_diagnostic(Diagnostic::new(DL_PENDING))
        .register_diagnostic(Diagnostic::new(DL_FINISHED))
        // .register_diagnostic(Diagnostic::new(IMAGE_COUNT))
        .register_diagnostic(Diagnostic::new(STDMAT_COUNT))
        .register_diagnostic(Diagnostic::new(MESH_COUNT))
        .add_systems(Startup, setup_diagnostic)
        .add_systems(Update, count_diagnostic);
    }
}

#[derive(Debug, Component, Default)]
pub struct DownloadPending;
#[derive(Debug, Component, Default)]
pub struct DownloadFinished;

fn setup_diagnostic(mut onscreen: ResMut<ScreenDiagnostics>) {
    onscreen
        .add("dl/pending;".to_string(), DL_PENDING)
        .aggregate(Aggregate::Value)
        .format(|v| format!("{v:.0}"));

    onscreen
        .add("dl/done;".to_string(), DL_FINISHED)
        .aggregate(Aggregate::Value)
        .format(|v| format!("{v:.0}"));

    // onscreen
    //     .add("images;".to_string(), IMAGE_COUNT)
    //     .aggregate(Aggregate::Value)
    //     .format(|v| format!("{v:.0}"));

    onscreen
        .add("std mat;".to_string(), STDMAT_COUNT)
        .aggregate(Aggregate::Value)
        .format(|v| format!("{v:.0}"));

    onscreen
        .add("meshes.".to_string(), MESH_COUNT)
        .aggregate(Aggregate::Value)
        .format(|v| format!("{v:.0}"));
}

fn count_diagnostic(
    mut diagnostics: Diagnostics,
    q_pending: Query<&DownloadPending>,
    q_finished: Query<&DownloadFinished>,
    q_meshes: Query<&Handle<Mesh>>,
    q_stdmat: Query<&Handle<StandardMaterial>>,
    // q_img: Query<&Handle<Image>>,
) {
    diagnostics.add_measurement(&DL_PENDING, || q_pending.iter().len() as f64);

    diagnostics
        .add_measurement(&DL_FINISHED, || q_finished.iter().len() as f64);

    diagnostics.add_measurement(&MESH_COUNT, || q_meshes.iter().len() as f64);

    diagnostics.add_measurement(&STDMAT_COUNT, || q_stdmat.iter().len() as f64);

    // diagnostics.add_measurement(&IMAGE_COUNT, || q_img.iter().len() as f64);
}
