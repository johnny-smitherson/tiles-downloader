use bevy::{
    diagnostic::{Diagnostic, DiagnosticPath, Diagnostics, RegisterDiagnostic},
    prelude::*,
};

use bevy_screen_diagnostics::{Aggregate, ScreenDiagnostics, ScreenDiagnosticsPlugin, ScreenEntityDiagnosticsPlugin, ScreenFrameDiagnosticsPlugin};

pub struct CustomDiagnosticsPlugin {}


const DL_PENDING: DiagnosticPath = DiagnosticPath::const_new("dl_pending");
const DL_FINISHED: DiagnosticPath = DiagnosticPath::const_new("dl_finished");

impl Plugin for CustomDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(
      (      ScreenDiagnosticsPlugin::default(),
            ScreenFrameDiagnosticsPlugin,
            ScreenEntityDiagnosticsPlugin,)
        )
        .register_diagnostic(Diagnostic::new(DL_PENDING))
        .register_diagnostic(Diagnostic::new(DL_FINISHED))
        
        .add_systems(Startup, setup_diagnostic)
        .add_systems(Update, count_diagnostic)
        ;
    }
}



#[derive(Debug, Component, Default)]
pub struct DownloadPending;
#[derive(Debug, Component, Default)]
pub struct DownloadFinished;



fn setup_diagnostic(mut onscreen: ResMut<ScreenDiagnostics>) {
    onscreen
        .add("dl/pending".to_string(), DL_PENDING)
        .aggregate(Aggregate::Value)
        .format(|v| format!("{v:.0}"));
    onscreen
    .add("dl/done".to_string(), DL_FINISHED)
    .aggregate(Aggregate::Value)
    .format(|v| format!("{v:.0}"));
}

fn count_diagnostic(mut diagnostics: Diagnostics, q_pending: Query<&DownloadPending>, q_finished: Query<&DownloadFinished>) {
    diagnostics.add_measurement(&DL_PENDING, || q_pending.iter().len() as f64);
    diagnostics.add_measurement(&DL_FINISHED, || q_finished.iter().len() as f64);
}


