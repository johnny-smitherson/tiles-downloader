//! A simple 3D scene with light shining over a cube sitting on a plane.

use crate::geo_trig;
use crate::input_events::CameraMoveEvent;
use bevy::math::DVec3;
use bevy::prelude::*;
use big_space::GridCell;

pub struct EarthCameraPlugin {}

impl Plugin for EarthCameraPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<EarthCamera>()
            .add_systems(Update, read_camera_input_events);
    }
}

#[derive(Debug, Component, Reflect, Clone)]
pub struct EarthCamera {
    geo_x_deg: f64,
    geo_y_deg: f64,
    geo_alt: f64,
    max_camera_alt: f64,
    min_camera_alt: f64,
}

const MAX_CAMERA_Y_DEG: f64 = 84.0;

impl EarthCamera {
    pub fn get_abs_transform(&self) -> (Transform, DVec3) {
        let xyz = geo_trig::gps_to_cartesian(self.geo_x_deg, self.geo_y_deg)
            .normalize()
            * (self.min_camera_alt + self.geo_alt);
        let tr = Transform::from_translation(xyz.as_vec3())
            .looking_at(Vec3::ZERO, Vec3::Y);
        (tr, xyz)
    }

    fn limit_fields(&mut self) {
        let epsilon: f64 = 1.0 / self.min_camera_alt; // 1m where 1.0 is radius of planet
        if self.geo_alt < epsilon {
            self.geo_alt = epsilon;
        }
        if self.geo_alt > self.max_camera_alt {
            self.geo_alt = self.max_camera_alt;
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
        let speed = 15.0 * self.geo_alt / self.min_camera_alt;
        let x_speed = speed / self.geo_y_deg.to_radians().cos();
        let y_speed = speed;
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
                self.geo_alt *= 1.0 - z_exp_speed * ev.value;
            }
            crate::input_events::CameraMoveDirection::ZOOMOUT => {
                self.geo_alt *= 1.0 + z_exp_speed * ev.value;
            }
        }
    }
}

impl EarthCamera {
    pub fn from_planet_radius(planet_radius: f64) -> Self {
        let mut x = Self {
            geo_x_deg: -83.0458,
            geo_y_deg: 42.3314,
            geo_alt: planet_radius * 2.,
            min_camera_alt: planet_radius + 1.0,
            max_camera_alt: planet_radius * 3.,
        };
        x.limit_fields();
        x
    }
}

fn read_camera_input_events(
    mut camera_events: EventReader<CameraMoveEvent>,
    mut camera_q: Query<(&mut EarthCamera, &mut Transform, &mut GridCell<i64>, &Parent)>,
    space_q: Query<&big_space::reference_frame::ReferenceFrame<i64>>,
) {
    let events: Vec<_> = camera_events.read().collect();
    if events.is_empty() {
        return;
    }
    for (mut cam, mut transform, mut cell, cam_p) in camera_q.iter_mut() {
        let space = space_q.get(cam_p.get()).unwrap();
        // let old_transform = cam.get_abs_transform();
        for ev in events.iter() {
            cam.accept_event(ev);
            cam.limit_fields();
        }
        let (tr, xyz) = cam.get_abs_transform();
        let (new_cell, crop_tr) = space.translation_to_grid(xyz);
        *cell = new_cell;
        transform.translation = crop_tr;
        transform.rotation = tr.rotation;
    }
}

// #[derive(Component)]
// struct MinimapCamera;

// #[allow(clippy::needless_pass_by_value)]
// fn resize_minimap(
//     windows: Query<&Window>,
//     mut resize_events: EventReader<bevy::window::WindowResized>,
//     mut minimap: Query<&mut Camera, With<MinimapCamera>>,
// ) {
//     for resize_event in resize_events.read() {
//         let window = windows.get(resize_event.window).unwrap();
//         let mut minimap = minimap.single_mut();
//         let size = window.resolution.physical_width() / 4;
//         minimap.viewport = Some(bevy::render::camera::Viewport {
//             physical_position: UVec2::new(
//                 window.resolution.physical_width() - size,
//                 window.resolution.physical_height() - size,
//             ),
//             physical_size: UVec2::new(size, size),
//             ..default()
//         });
//     }
// }
