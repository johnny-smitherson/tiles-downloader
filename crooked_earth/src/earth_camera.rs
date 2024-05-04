//! A simple 3D scene with light shining over a cube sitting on a plane.

use crate::geo_trig;
use crate::input_events::CameraMoveEvent;
use bevy::prelude::*;
use bevy_trackball::TrackballController;

pub struct EarthCameraPlugin {}

impl Plugin for EarthCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera);
        app.add_systems(Update, read_camera_input_events)
            .add_systems(Update, resize_minimap);
    }
}

#[derive(Debug, Component, Reflect)]
pub struct EarthCamera {
    geo_x_deg: f64,
    geo_y_deg: f64,
    geo_alt_km: f64,
}

#[derive(Debug, Component, Reflect, Default)]
pub struct Sun;

pub const EARTH_RADIUS_KM: f64 = 6378.137;
const MAX_CAMERA_ALT_KM: f64 = EARTH_RADIUS_KM * 2.0;
const MAX_CAMERA_Y_DEG: f64 = 84.0;

impl EarthCamera {
    fn get_transform(&self) -> Transform {
        let xyz = geo_trig::gps_to_cartesian(self.geo_x_deg, self.geo_y_deg)
            .normalize()
            * (EARTH_RADIUS_KM + self.geo_alt_km) as f32;
        Transform::from_translation(xyz).looking_at(Vec3::ZERO, Vec3::Y)
    }
    fn get_sun(&self) -> Transform {
        let xyz = geo_trig::gps_to_cartesian(
            self.geo_x_deg + 35.0,
            self.geo_y_deg / 3.0,
        )
        .normalize();
        Transform::from_translation(xyz).looking_at(Vec3::ZERO, Vec3::Y)
    }
    fn limit_fields(&mut self) {
        const EPSILON: f64 = 1.0 / EARTH_RADIUS_KM / 1000.0; // 1m where 1.0 is radius of planet
        if self.geo_alt_km < EPSILON {
            self.geo_alt_km = EPSILON;
        }
        if self.geo_alt_km > MAX_CAMERA_ALT_KM {
            self.geo_alt_km = MAX_CAMERA_ALT_KM;
        }
        while self.geo_x_deg < -180.0 {
            self.geo_x_deg += 360.0;
        }
        while self.geo_x_deg > 180.0 {
            self.geo_x_deg -= 360.0;
        }
        if self.geo_y_deg > MAX_CAMERA_Y_DEG {
            self.geo_y_deg = MAX_CAMERA_Y_DEG;
        }
        if self.geo_y_deg < -MAX_CAMERA_Y_DEG {
            self.geo_y_deg = -MAX_CAMERA_Y_DEG;
        }
    }
    fn accept_event(&mut self, ev: &CameraMoveEvent) {
        let speed = 15.0 / EARTH_RADIUS_KM;
        let x_speed =
            speed * self.geo_alt_km / self.geo_y_deg.to_radians().cos();
        let y_speed = speed * self.geo_alt_km;
        let z_exp_speed = 0.3;
        match ev.direction {
            crate::input_events::CameraMoveDirection::UP => {
                self.geo_y_deg += ev.value * y_speed;
            }
            crate::input_events::CameraMoveDirection::DOWN => {
                self.geo_y_deg -= ev.value * y_speed;
            }
            crate::input_events::CameraMoveDirection::LEFT => {
                self.geo_x_deg -= ev.value * x_speed;
            }
            crate::input_events::CameraMoveDirection::RIGHT => {
                self.geo_x_deg += ev.value * x_speed;
            }
            crate::input_events::CameraMoveDirection::ZOOMIN => {
                self.geo_alt_km *= 1.0 - z_exp_speed * ev.value;
            }
            crate::input_events::CameraMoveDirection::ZOOMOUT => {
                self.geo_alt_km *= 1.0 + z_exp_speed * ev.value;
            }
        }
    }
}

impl Default for EarthCamera {
    fn default() -> Self {
        let mut x = Self {
            geo_x_deg: -83.0458,
            geo_y_deg: 42.3314,
            geo_alt_km: MAX_CAMERA_ALT_KM,
        };
        x.limit_fields();
        x
    }
}

fn read_camera_input_events(
    mut camera_events: EventReader<CameraMoveEvent>,
    mut camera_q: Query<(&mut EarthCamera, &mut Transform), Without<Sun>>,
    mut sun_q: Query<&mut Transform, With<Sun>>,
) {
    let events: Vec<_> = camera_events.read().collect();
    if events.is_empty() {
        return;
    }
    for (mut cam, mut transform) in camera_q.iter_mut() {
        for ev in events.iter() {
            cam.accept_event(ev);
            cam.limit_fields();
        }
        *transform = cam.get_transform();
        for mut sun_tr in sun_q.iter_mut() {
            *sun_tr = cam.get_sun();
        }
    }
}

fn setup_camera(mut commands: Commands, windows: Query<&Window>) {
    let c = EarthCamera::default();
    let c_t = c.get_transform();
    let s_t = c.get_sun();
    commands.spawn((
        // bevy_trackball::TrackballController::default(),
        // bevy_trackball::TrackballCamera::look_at(target, eye, up),
        Camera3dBundle {
            transform: c_t,
            ..default()
        },
        c,
    ));

    commands.spawn((
        DirectionalLightBundle {
            directional_light: DirectionalLight {
                shadows_enabled: true,
                ..default()
            },
            transform: s_t,
            ..default()
        },
        Sun::default(),
    ));

    use bevy_trackball::prelude::Bound;
    use bevy_trackball::TrackballCamera;

    let mut bound = Bound::<f32>::default();
    bound.min_target = [-EARTH_RADIUS_KM as f32; 3].into();
    bound.max_target = [EARTH_RADIUS_KM as f32; 3].into();

    bound.min_distance = 1.0;
    bound.max_distance = EARTH_RADIUS_KM as f32 * 3.0;
    let window = windows.single();
    let size = window.resolution.physical_width() / 4;

    commands.spawn((
        get_trackball_controller_no_keys(),
        TrackballCamera::look_at(Vec3::ZERO, c_t.translation, Vec3::Y)
            .with_clamp(bound),
        Camera3dBundle {
            camera: Camera {
                order: 1,
                clear_color: ClearColorConfig::None,
                viewport: Some(bevy::render::camera::Viewport {
                    physical_position: UVec2::new(
                        window.resolution.physical_width() - size,
                        window.resolution.physical_height() - size,
                    ),
                    physical_size: UVec2::new(size, size),
                    ..default()
                }),
                ..default()
            },
            ..default()
        },
        MinimapCamera,
    ));
}

fn get_trackball_controller_no_keys() -> TrackballController {
    use bevy_trackball::TrackballInput;
    use bevy_trackball::TrackballVelocity;
    use bevy_trackball::TrackballWheelUnit;
    let mut trackball_controller = TrackballController::default();
    trackball_controller.input = TrackballInput {
        velocity: TrackballVelocity::default(),
        wheel_unit: TrackballWheelUnit::default(),

        focus: true,

        gamer_key: None,
        ortho_key: Some(KeyCode::KeyP),

        reset_key: Some(KeyCode::Enter),

        first_key: None,
        first_button: Some(MouseButton::Middle),
        first_left_key: Some(KeyCode::ArrowLeft),
        first_right_key: Some(KeyCode::ArrowRight),
        first_up_key: Some(KeyCode::ArrowUp),
        first_down_key: Some(KeyCode::ArrowDown),

        orbit_button: Some(MouseButton::Left),
        screw_left_key: None,
        screw_right_key: None,
        orbit_left_key: None,
        orbit_right_key: None,
        orbit_up_key: None,
        orbit_down_key: None,

        slide_button: Some(MouseButton::Right),
        slide_up_key: None,
        slide_down_key: None,
        slide_left_key: None,
        slide_right_key: None,
        slide_far_key: None,
        slide_near_key: None,

        scale_in_key: None,
        scale_out_key: None,
    };
    trackball_controller
}

#[derive(Component)]
struct MinimapCamera;

#[allow(clippy::needless_pass_by_value)]
fn resize_minimap(
    windows: Query<&Window>,
    mut resize_events: EventReader<bevy::window::WindowResized>,
    mut minimap: Query<&mut Camera, With<MinimapCamera>>,
) {
    for resize_event in resize_events.read() {
        let window = windows.get(resize_event.window).unwrap();
        let mut minimap = minimap.single_mut();
        let size = window.resolution.physical_width() / 4;
        minimap.viewport = Some(bevy::render::camera::Viewport {
            physical_position: UVec2::new(
                window.resolution.physical_width() - size,
                window.resolution.physical_height() - size,
            ),
            physical_size: UVec2::new(size, size),
            ..default()
        });
    }
}
