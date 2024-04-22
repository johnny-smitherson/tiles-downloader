use anyhow::Result;
use image::DynamicImage;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use image::io::Reader as ImageReader;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

use geojson::FeatureCollection;

use crate::config;
use crate::config::{TileServerConfig, LINKS_CONFIG};
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
    fn get_version(&self) -> usize {
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
            crate::geo_trig::xyz_to_bing_quadkey(self.x, self.y, self.z),
        );

        let url =
            strfmt::strfmt(&server_config.url, &map).context("failed strfmt on URL");
        Ok(url?)
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
    Ok(fetch_info.get_final_path()?)
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
struct OSMGeolocationSearchQuery {
    query_str: String,
}

#[derive(
    Copy, Deserialize, Clone, Debug, Serialize, PartialEq, FromForm, UriDisplayQuery,
)]
pub struct GeoPoint {
    pub point_0: f64,
    pub point_1: f64,
}

#[derive(
    Copy, Deserialize, Clone, Debug, Serialize, PartialEq, FromForm, UriDisplayQuery,
)]
pub struct GeoBBOX {
    pub bbox_0: f64,
    pub bbox_1: f64,
    pub bbox_2: f64,
    pub bbox_3: f64,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct OSMGeolocationSearchResult {
    pub display_name: String,
    pub geo_point: GeoPoint,
    pub bbox: GeoBBOX,
}

impl DownloadId for OSMGeolocationSearchQuery {
    type TParseResult = Vec<OSMGeolocationSearchResult>;
    fn get_version(&self) -> usize {
        return 0;
    }
    fn is_valid_request(&self) -> Result<()> {
        if self.query_str.len() >= 2 && self.query_str.len() <= 128 {
            return Ok(());
        }
        anyhow::bail!(
            "bad query {}: too short (<2) or too long (>128)",
            self.query_str
        )
    }
    fn get_final_path(&self) -> Result<PathBuf> {
        let query_urlencode =
            urlencoding::encode(self.query_str.as_str()).into_owned();
        let dir_path = LINKS_CONFIG.tile_location.join("geojson");
        let path = dir_path.join(format!("{}.geo.json", query_urlencode));
        Ok(path)
    }
    fn get_random_url(&self) -> Result<String> {
        let url = {
            let query_urlencode =
                urlencoding::encode(self.query_str.as_str()).into_owned();
            let mut map: HashMap<String, String> = HashMap::with_capacity(10);
            map.insert("q_urlencoded".to_owned(), query_urlencode.clone());

            strfmt::strfmt(&LINKS_CONFIG.geo_search_url, &map)
                .context("failed strfmt on URL")?
        };
        Ok(url)
    }
    fn parse_respose(&self, tmp_file: &Path) -> Result<Self::TParseResult> {
        let bytes = std::fs::read(tmp_file)?;
        // let _data: serde_json::Value = serde_json::from_slice(&bytes)?;
        let geo_collection: FeatureCollection = serde_json::from_slice(&bytes)?;
        if geo_collection.features.is_empty() {
            return Ok(vec![]);
        }
        let mut data = vec![];
        for feature in geo_collection.features.iter() {
            let geo_point = &feature.geometry.clone().context("no geometry?")?.value;
            let geo_point = {
                if let geojson::Value::Point(coords) = geo_point {
                    (coords[0], coords[1])
                } else {
                    return Err(anyhow::anyhow!("geometry was not point - ").into());
                }
            };
            let geo_point = GeoPoint {
                point_0: geo_point.0,
                point_1: geo_point.1,
            };

            let bbox = feature.bbox.clone().context("no bbox")?;
            let bbox = GeoBBOX {
                bbox_0: bbox[0],
                bbox_1: bbox[1],
                bbox_2: bbox[2],
                bbox_3: bbox[3],
            };

            let display_name = feature
                .properties
                .clone()
                .context("no properties")?
                .get("display_name")
                .context("no display name?")?
                .clone()
                .to_string();
            data.push(OSMGeolocationSearchResult {
                bbox,
                geo_point,
                display_name,
            });
        }
        Ok(data)
    }
}

pub async fn search_geojson_to_disk(query_str: &str) -> Result<std::path::PathBuf> {
    let download_id = OSMGeolocationSearchQuery {
        query_str: query_str.to_string(),
    };
    crate::proxy_manager::download2(&download_id).await?;
    Ok(download_id.get_final_path()?)
}

pub async fn search_geojson(
    query_str: &str,
) -> Result<Vec<OSMGeolocationSearchResult>> {
    let download_id = OSMGeolocationSearchQuery {
        query_str: query_str.to_string(),
    };
    let res = crate::proxy_manager::download2(&download_id).await?;
    Ok(res)
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

    use image::io::Reader as ImageReader;

    let img_reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let img = img_reader.decode()?;
    let image_format = match img_type {
        "png" => image::ImageFormat::Png,
        "jpg" => image::ImageFormat::Jpeg,
        _ => anyhow::bail!("bad format: {}", img_type),
    };
    let b_px = overlay_coordinates.point.context("no point coord!")?;
    let b_px = crate::geo_trig::tile_index_float(z, b_px.point_0, b_px.point_1);

    let tile2pixel = |point: (f64, f64)| {
        (
            ((point.0 - x as f64) * server_config.width as f64) as i32,
            ((point.1 - y as f64) * server_config.width as f64) as i32,
        )
    };
    let b_px = tile2pixel(b_px);

    let b_bbox = overlay_coordinates.bbox.context("no bbox")?;
    let bbox0 = crate::geo_trig::tile_index_float(z, b_bbox.bbox_0, b_bbox.bbox_1);
    let bbox1 = crate::geo_trig::tile_index_float(z, b_bbox.bbox_2, b_bbox.bbox_3);
    let bbox0 = tile2pixel(bbox0);
    let bbox1 = tile2pixel(bbox1);
    let b_bbox = [bbox0, bbox1, (bbox1.0, bbox0.1), (bbox0.0, bbox1.1)];

    eprintln!("point: {:?}  bbox: {:?}", b_px, b_bbox);

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
