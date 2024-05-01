use bevy::prelude::*;

pub struct InputEventsPlugin {}

impl Plugin for InputEventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, keyboard_input_system);
        app.add_event::<CameraMoveEvent>();
    }
}

#[derive(Debug)]
pub enum CameraMoveDirection {
    UP,
    DOWN,
    LEFT,
    RIGHT,
    ZOOMIN,
    ZOOMOUT,
}

#[derive(Event, Debug)]
pub struct CameraMoveEvent {
    pub direction: CameraMoveDirection,
    pub value: f64,
}

const KEYBOARD_INPUT_VALUE: f64 = 1.0;

fn keyboard_input_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut events: EventWriter<CameraMoveEvent>,
    time: Res<Time>,
) {
    let dt_cap = time.delta_seconds_f64().min(1.0 / 30.0);
    let kb_val = dt_cap * KEYBOARD_INPUT_VALUE;
    if keyboard_input.pressed(KeyCode::KeyA)
        || keyboard_input.pressed(KeyCode::ArrowLeft)
    {
        events.send(CameraMoveEvent {
            direction: CameraMoveDirection::LEFT,
            value: kb_val,
        });
    }

    if keyboard_input.pressed(KeyCode::KeyD)
        || keyboard_input.pressed(KeyCode::ArrowRight)
    {
        events.send(CameraMoveEvent {
            direction: CameraMoveDirection::RIGHT,
            value: kb_val,
        });
    }

    if keyboard_input.pressed(KeyCode::KeyW)
        || keyboard_input.pressed(KeyCode::ArrowUp)
    {
        events.send(CameraMoveEvent {
            direction: CameraMoveDirection::UP,
            value: kb_val,
        });
    }

    if keyboard_input.pressed(KeyCode::KeyS)
        || keyboard_input.pressed(KeyCode::ArrowDown)
    {
        events.send(CameraMoveEvent {
            direction: CameraMoveDirection::DOWN,
            value: kb_val,
        });
    }

    if keyboard_input.pressed(KeyCode::ShiftLeft)
        || keyboard_input.pressed(KeyCode::ShiftRight)
    {
        events.send(CameraMoveEvent {
            direction: CameraMoveDirection::ZOOMIN,
            value: kb_val,
        });
    }

    if keyboard_input.pressed(KeyCode::ControlLeft)
        || keyboard_input.pressed(KeyCode::ControlRight)
    {
        events.send(CameraMoveEvent {
            direction: CameraMoveDirection::ZOOMOUT,
            value: kb_val,
        });
    }
}
