use std::f64::consts::PI;

use bevy::prelude::*;



pub struct InputEventsPlugin {

}

impl Plugin for InputEventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update,keyboard_input_system);
    }
}



/// This system prints 'A' key state
fn keyboard_input_system(keyboard_input: Res<ButtonInput<KeyCode>>) {
    if keyboard_input.pressed(KeyCode::KeyA) {
        info!("'A' currently pressed");
    }

    if keyboard_input.just_pressed(KeyCode::KeyA) {
        info!("'A' just pressed");
    }
    if keyboard_input.just_released(KeyCode::KeyA) {
        info!("'A' just released");
    }
}