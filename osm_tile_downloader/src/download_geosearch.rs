use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use geojson::FeatureCollection;

use crate::config::LINKS_CONFIG;
use crate::proxy_manager::download2;
use crate::proxy_manager::DownloadId;

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

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
struct OSMGeolocationSearchQuery {
    query_str: String,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct OSMGeolocationSearchResult {
    pub display_name: String,
    pub geo_point: GeoPoint,
    pub bbox: GeoBBOX,
}

impl DownloadId for OSMGeolocationSearchQuery {
    type TParseResult = Vec<OSMGeolocationSearchResult>;
    fn get_version() -> usize {
        0
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
                    return Err(anyhow::anyhow!("geometry was not point - "));
                }
            };
            let geo_point = GeoPoint {
                x_lon: geo_point.0,
                y_lat: geo_point.1,
            };

            let bbox = feature.bbox.clone().context("no bbox")?;
            let bbox = GeoBBOX {
                x_min: bbox[0],
                y_min: bbox[1],
                x_max: bbox[2],
                y_max: bbox[3],
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
    download2(&download_id).await?;
    download_id.get_final_path()
}

pub async fn search_geojson(
    query_str: &str,
) -> Result<Vec<OSMGeolocationSearchResult>> {
    let download_id = OSMGeolocationSearchQuery {
        query_str: query_str.to_string(),
    };
    let res = download2(&download_id).await?;
    Ok(res)
}
