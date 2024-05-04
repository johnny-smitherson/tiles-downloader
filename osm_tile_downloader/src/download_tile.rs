use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use image::io::Reader as ImageReader;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

use crate::config;
use crate::config::{TileServerConfig, LINKS_CONFIG};
use crate::geo_trig::tile_index_float;
use crate::geo_trig::xyz_to_bing_quadkey;
use crate::geo_trig::{GeoBBOX, GeoPoint};
use crate::proxy_manager;
use crate::proxy_manager::DownloadId;

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct TileFetchId {
    pub x: u64,
    pub y: u64,
    pub z: u8,
    pub server_name: String,
    pub extension: String,
}

impl TileFetchId {
    fn get_server_config(&self) -> Result<TileServerConfig> {
        config::get_tile_server(&self.server_name)
    }
}

impl DownloadId for TileFetchId {
    type TParseResult = ();
    fn get_max_parallel() -> i64 {
        222
    }
    fn get_version() -> usize {
        0
    }

    fn is_valid_request(&self) -> Result<()> {
        let server_config = self.get_server_config()?;

        if server_config.max_level < self.z {
            anyhow::bail!(
                "got z = {} when max for server is {}",
                self.z,
                server_config.max_level
            );
        };

        if !(self.extension.eq(&server_config.img_type)) {
            anyhow::bail!(
                "got extension = {} when server img_type is {}",
                &self.extension,
                &server_config.img_type
            );
        };
        let max_extent = 2u64.pow(self.z.into()) - 1;
        if !(self.x <= max_extent && self.y <= max_extent) {
            anyhow::bail!(
                "x={}, y={} not inside extent={} for z={}",
                self.x,
                self.y,
                max_extent,
                self.z
            );
        }
        Ok(())
    }
    fn get_final_path(&self) -> anyhow::Result<PathBuf> {
        let server_config = self.get_server_config()?;

        assert!(server_config.name.eq(&self.server_name));
        assert!(server_config.img_type.eq(&self.extension));
        let mut target = LINKS_CONFIG
            .tile_location
            .clone()
            .join(&server_config.map_type)
            .join(&server_config.name)
            .join(self.z.to_string())
            .join(self.x.to_string());
        target.push(format!("{}.{}", self.y, self.extension));

        Ok(target)
    }

    fn get_random_url(&self) -> anyhow::Result<String> {
        use rand::seq::SliceRandom;
        use std::collections::HashMap;

        let mut map: HashMap<String, String> = HashMap::with_capacity(10);
        let server_config = self.get_server_config()?;
        let server_bit = {
            if server_config.servers.is_none() {
                "".to_owned()
            } else {
                server_config
                    .servers
                    .as_ref()
                    .context("empty server letter")?
                    .choose(&mut rand::thread_rng())
                    .context("empty server vector")?
                    .to_owned()
            }
        };

        map.insert("s".to_owned(), server_bit);
        map.insert("x".to_owned(), self.x.to_string());
        map.insert("y".to_owned(), self.y.to_string());
        map.insert("z".to_owned(), self.z.to_string());
        map.insert(
            "bing_quadkey".to_owned(),
            xyz_to_bing_quadkey(self.x, self.y, self.z),
        );

        strfmt::strfmt(&server_config.url, &map).context("failed strfmt on URL")
    }

    fn parse_respose(&self, tmp_file: &Path) -> Result<Self::TParseResult> {
        let img_path = PathBuf::from(tmp_file);
        let server_config = self.get_server_config()?;
        // let bytes = tokio::fs::read(img_path).await?;
        let bytes = std::fs::read(img_path)?;
        let img_reader =
            ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
        let img_format = img_reader
            .format()
            .context("auto-guesser failed to get img format")?;
        let found_extension = img_format
            .extensions_str()
            .iter()
            .any(|&x| *x == server_config.img_type);
        if !found_extension {
            anyhow::bail!("did not find our expected tile server extension = {:?} \n in list of auto-detected extensions = {:?}", server_config.img_type, img_format.extensions_str());
        }
        let img = img_reader.decode()?;
        if img.width() != server_config.width {
            anyhow::bail!(
                "image width not correct, expected {}, got {}",
                server_config.width,
                img.width()
            );
        }
        if img.height() != server_config.height {
            anyhow::bail!(
                "image width not correct, expected {}, got {}",
                server_config.height,
                img.height()
            );
        }
        Ok(())
    }
}

pub async fn get_tile(
    server_name: &str,
    x: u64,
    y: u64,
    z: u8,
    extension: &str,
) -> Result<PathBuf> {
    let fetch_info = TileFetchId {
        x,
        y,
        z,
        server_name: server_name.to_owned(),
        extension: extension.to_owned(),
    };
    proxy_manager::download2(&fetch_info).await?;
    fetch_info.get_final_path()
}

use tokio::task::spawn_blocking;

#[derive(FromForm, UriDisplayQuery)]
pub struct OverlayDrawCoordinates {
    pub point: Option<GeoPoint>,
    pub bbox: Option<GeoBBOX>,
}

pub async fn draw_overlay_on_tile(
    x: u64,
    y: u64,
    z: u8,
    img_type: &str,
    path: &Path,
    overlay_coordinates: &OverlayDrawCoordinates,
    server_config: &TileServerConfig,
) -> Result<Vec<u8>> {
    let bytes = tokio::fs::read(path).await?;

    let img_reader =
        ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let img = img_reader.decode()?;
    let image_format = match img_type {
        "png" => image::ImageFormat::Png,
        "jpg" => image::ImageFormat::Jpeg,
        _ => anyhow::bail!("bad format: {}", img_type),
    };
    let b_px = overlay_coordinates.point.context("no point coord!")?;
    let b_px = tile_index_float(z, b_px.x_lon, b_px.y_lat);

    let tile2pixel = |point: (f64, f64)| {
        (
            ((point.0 - x as f64) * server_config.width as f64) as i32,
            ((point.1 - y as f64) * server_config.width as f64) as i32,
        )
    };
    let b_px = tile2pixel(b_px);

    let b_bbox = overlay_coordinates.bbox.context("no bbox")?;
    let bbox0 = tile_index_float(z, b_bbox.x_min, b_bbox.y_min);
    let bbox1 = tile_index_float(z, b_bbox.x_max, b_bbox.y_max);
    let bbox0 = tile2pixel(bbox0);
    let bbox1 = tile2pixel(bbox1);
    let b_bbox = [bbox0, bbox1, (bbox1.0, bbox0.1), (bbox0.0, bbox1.1)];

    // eprintln!("point: {:?}  bbox: {:?}", b_px, b_bbox);

    let img_bytes = spawn_blocking(move || {
        let mut img = img.into_rgb8();
        // let b_px: (i32, i32) = (127, 127);
        // let b_bbox: (i32, i32, i32, i32) = (32, 32, 172, 172);
        let line_len: i32 = 10;
        for pixel in img.enumerate_pixels_mut() {
            let current_pixel = (pixel.0 as i32, pixel.1 as i32);

            let hit_point_cross = |cxx: (i32, i32)| {
                (current_pixel.0 - cxx.0 == current_pixel.1 - cxx.1
                    && (current_pixel.0 - cxx.0).abs() <= line_len)
                    || (current_pixel.0 - cxx.0 == -current_pixel.1 + cxx.1
                        && (current_pixel.0 - cxx.0).abs() <= line_len)
            };

            if hit_point_cross(b_px) {
                *pixel.2 = pixel_max_contrast(pixel.2);
            }
            if current_pixel.0 == b_bbox[0].0
                || current_pixel.0 == b_bbox[1].0
                || current_pixel.1 == b_bbox[0].1
                || current_pixel.1 == b_bbox[1].1
            {
                *pixel.2 = pixel_max_contrast(pixel.2);
            }
        }

        let mut img_bytes: Vec<u8> = Vec::new();
        img.write_to(&mut Cursor::new(&mut img_bytes), image_format)
            .unwrap();
        img_bytes
    })
    .await?;
    Ok(img_bytes)
}

fn pixel_max_contrast(px: &image::Rgb<u8>) -> image::Rgb<u8> {
    image::Rgb::<u8>([
        if px.0[0] > 127 { 0 } else { 255 },
        if px.0[1] > 127 { 0 } else { 255 },
        if px.0[2] > 127 { 0 } else { 255 },
    ])
}
