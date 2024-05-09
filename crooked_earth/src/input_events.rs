use bevy::input::mouse::MouseWheel;use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
pub struct InputEventsPlugin {}

impl Plugin for InputEventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (mouse_wheel_input_system, keyboard_input_system, mouse_button_drag_moves_camera),
        );
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

const MOUSE_SCROLL_INPUT_VALUE: f64 = 25.0;

fn mouse_wheel_input_system(
    mut scroll_evr: EventReader<MouseWheel>,
    mut events: EventWriter<CameraMoveEvent>,
    time: Res<Time>,
) {
    let dt_cap = time.delta_seconds_f64().min(1.0 / 30.0);
    let mouse_val = dt_cap * MOUSE_SCROLL_INPUT_VALUE;
    use bevy::input::mouse::MouseScrollUnit;
    for ev in scroll_evr.read() {
        match ev.unit {
            MouseScrollUnit::Line => {
                events.send(CameraMoveEvent {
                    direction: CameraMoveDirection::ZOOMIN,
                    value: ev.y as f64 * mouse_val,
                });
            }
            MouseScrollUnit::Pixel => {
                events.send(CameraMoveEvent {
                    direction: CameraMoveDirection::ZOOMIN,
                    value: ev.y as f64 * mouse_val / 25.0,
                });
            }
        }
    }
}

const MOUSE_DRAG_INPUT_VALUE: f64 = 0.3;

fn mouse_button_drag_moves_camera(
    mut motion_evr: EventReader<MouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut events: EventWriter<CameraMoveEvent>,
    time: Res<Time>,
) {    
    let dt_cap = time.delta_seconds_f64().min(1.0 / 30.0);
    let drag_val = dt_cap * MOUSE_DRAG_INPUT_VALUE;
    if buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Left) {
        for ev in motion_evr.read() {
            events.send(CameraMoveEvent {
                direction: CameraMoveDirection::LEFT,
                value: ev.delta.x as f64 * drag_val,
            });
            events.send(CameraMoveEvent {
                direction: CameraMoveDirection::UP,
                value: ev.delta.y as f64 * drag_val,
            });
        }
    }
}

const KEYBOARD_INPUT_VALUE: f64 = 1.5;

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
