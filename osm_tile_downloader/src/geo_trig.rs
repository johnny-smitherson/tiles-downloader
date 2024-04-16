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
