use serde::{Deserialize, Serialize};

#[derive(
    Copy, Deserialize, Clone, Debug, Serialize, PartialEq, FromForm, UriDisplayQuery,
)]
pub struct GeoPoint {
    pub x_lon: f64,
    pub y_lat: f64,
}

#[derive(
    Copy, Deserialize, Clone, Debug, Serialize, PartialEq, FromForm, UriDisplayQuery,
)]
pub struct GeoBBOX {
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
}

impl GeoBBOX {
    pub fn expand_relative(&self, q: f64) -> Self {
        let dx = self.x_max - self.x_min;
        let dy = self.y_max - self.y_min;
        assert!(dx > 0.0 && dy > 0.0, "malformed bbox");
        Self {
            x_min: self.x_min - dx,
            x_max: self.x_max + dx,
            y_min: self.y_min - dy,
            y_max: self.y_max + dy,
        }
    }
}

pub fn tile_index(zoom: u8, lon_deg: f64, lat_deg: f64) -> (u64, u64) {
    let (tile_x, tile_y) = tile_index_float(zoom, lon_deg, lat_deg);

    (tile_x as u64, tile_y as u64)
}

pub fn tile_index_float(zoom: u8, lon_deg: f64, lat_deg: f64) -> (f64, f64) {
    let lon_rad = lon_deg.to_radians();
    let lat_rad = lat_deg.to_radians();

    let tile_x = {
        let deg = (lon_rad + std::f64::consts::PI) / (2f64 * std::f64::consts::PI);

        deg * 2f64.powi(zoom as i32)
    };
    let tile_y = {
        let trig = (lat_rad.tan() + (1f64 / lat_rad.cos())).ln();
        let inner = 1f64 - (trig / std::f64::consts::PI);

        inner * 2f64.powi(zoom as i32 - 1)
    };
    (tile_x, tile_y)
}

// https://stackoverflow.com/questions/32454234/using-bing-maps-quadkeys-as-openlayers-3-tile-source
pub fn xyz_to_bing_quadkey(x: u64, y: u64, z: u8) -> String {
    // let y = -(y as i64) - 1;
    let mut quad_key = vec![];
    for i in (1..=z).rev() {
        let mut digit = 0;
        let mask = 1 << (i - 1);
        if (x & mask) != 0 {
            digit += 1
        }
        if (y & mask) != 0 {
            digit += 2
        }
        quad_key.push(digit.to_string().chars().next().unwrap());
    }
    quad_key.iter().collect()
}

pub fn geo_bbox(x: u64, y: u64, z: u8) -> GeoBBOX {
    use std::f64::consts::PI;
    GeoBBOX {
        x_min: (x as f64 / 2.0_f64.powi(z as i32)) * 360.0 - 180.0,
        x_max: ((x + 1) as f64 / 2.0_f64.powi(z as i32)) * 360.0 - 180.0,
        y_min: (PI - ((y + 1) as f64) / 2.0_f64.powi(z as i32) * 2.0 * PI)
            .sinh()
            .atan()
            * 180.0
            / PI,
        y_max: (PI - (y as f64) / 2.0_f64.powi(z as i32) * 2.0 * PI)
            .sinh()
            .atan()
            * 180.0
            / PI,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_index() {
        assert_eq!(
            tile_index(18, 6.0402f64.to_radians(), 50.7929f64.to_radians()),
            (135470, 87999)
        );
    }
}
