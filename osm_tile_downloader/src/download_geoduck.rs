use anyhow::Result;
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use std::path::PathBuf;

use crate::config::LINKS_CONFIG;
use crate::geo_trig::geo_bbox;
use crate::proxy_manager::{download2, DownloadId};
use serde::{Deserialize, Serialize};

lazy_static::lazy_static! {
    pub static ref OVERT_THEMES: Vec<(String, String)> = overt_geoduck::OVERT_TABLES.iter().filter(|x| !x.1.eq("*")).map(|(x, y)| (x.to_string(), y.to_string())).collect();
}
pub const GEODUCK_ZOOM_LEVEL: u8 = 10;
pub const PARQUET_MAX_ZOOM_LEVEL: u8 = 16;

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
struct OvertureMapsSegment {
    pub theme: String,
    pub _type: String,
    pub x: u64,
    pub y: u64,
    pub z: u8,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct GeoDuckSegmentSummary {
    pub feature_count: u64,
    pub size_mb: f64,
}

impl DownloadId for OvertureMapsSegment {
    type TParseResult = GeoDuckSegmentSummary;
    fn get_version() -> usize {
        3
    }
    fn get_max_parallel() -> i64 {
        6
    }
    fn parent(&self) -> Option<Self> {
        if self.z <= GEODUCK_ZOOM_LEVEL {
            return None;
        }
        Some(OvertureMapsSegment {
            theme: self.theme.clone(),
            _type: self._type.clone(),
            x: self.x / 2,
            y: self.y / 2,
            z: self.z - 1,
        })
    }
    fn is_valid_request(&self) -> Result<()> {
        if !OVERT_THEMES.contains(&(self.theme.to_owned(), self._type.to_owned())) {
            anyhow::bail!(
                "THEME/TYPE NOT FOUND: {}/{}, valid themes: {:?}",
                self.theme,
                self._type,
                OVERT_THEMES.as_slice()
            );
        }
        if self.z < GEODUCK_ZOOM_LEVEL || self.z > PARQUET_MAX_ZOOM_LEVEL {
            anyhow::bail!(
                "BAD ZOOM LEVEL {}, expected {}-{}",
                self.z,
                GEODUCK_ZOOM_LEVEL,
                PARQUET_MAX_ZOOM_LEVEL
            );
        }
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
    fn get_final_path(&self) -> Result<PathBuf> {
        let dir_path = LINKS_CONFIG
            .tile_location
            .join("geoduck")
            .join(&self.theme)
            .join(&self._type);
        let path = dir_path
            .join(self.z.to_string())
            .join(self.x.to_string())
            .join(self.y.to_string())
            .join("data.geo.parquet");
        Ok(path)
    }
    fn get_random_url(&self) -> Result<String> {
        Ok("".to_string())
    }
    fn parse_respose(&self, tmp_file: &Path) -> Result<Self::TParseResult> {
        let meta_size = std::fs::metadata(tmp_file)?.file_size();
        let size_mb = meta_size as f64 / 1024.0 / 1024.0;
        let size_mb = ((size_mb * 100.0) as i64) as f64 / 100.0;
        // let geo_collection: geojson::FeatureCollection =
        //     serde_json::from_slice(&bytes)?;
        // let feature_count = geo_collection.features.len() as u64;
        Ok(GeoDuckSegmentSummary {
            feature_count: 0,
            size_mb,
        })
    }
    fn download_into(
        &self,
        tmp_file: &Path,
    ) -> impl std::future::Future<Output = Result<Self::TParseResult>> + std::marker::Send
    {
        let tmp_file =
            std::path::PathBuf::from(tmp_file.to_str().unwrap().replace("\\", "/"));
        let tmp_file2 = std::path::PathBuf::from(&tmp_file);
        let tmp_file3 = std::path::PathBuf::from(&tmp_file);
        let bbox = geo_bbox(self.x, self.y, self.z).expand_relative(0.52);
        eprintln!("downloading geoduck {:?} into: {:?}", &self, tmp_file);
        async move {
            let theme2 = self.theme.clone();
            let type2 = self._type.clone();
            let maybe_parent_path = if let Some(parent) = self.parent() {
                Some(parent.get_final_path()?)
            } else {
                None
            };

            let rv = tokio::task::spawn_blocking(move || {
                if let Some(parent) = maybe_parent_path {
                    overt_geoduck::crop_geoparquet(
                        &parent, bbox.x_min, bbox.x_max, bbox.y_min, bbox.y_max,
                        &tmp_file2,
                    )
                } else {
                    overt_geoduck::download_geoparquet(
                        &theme2, &type2, bbox.x_min, bbox.x_max, bbox.y_min,
                        bbox.y_max, &tmp_file2,
                    )
                }
            })
            .await?;
            eprintln!("geoduck {:?} download result = {:?}", &self, rv);
            rv?;

            self.parse_respose(&tmp_file3.clone())
        }
    }
    fn get_retry_count() -> u8 {
        2
    }
}

pub async fn download_geoduck_to_disk(
    theme: &str,
    _type: &str,
    x: u64,
    y: u64,
    z: u8,
) -> anyhow::Result<std::path::PathBuf> {
    let download_id = OvertureMapsSegment {
        theme: theme.to_string(),
        _type: _type.to_string(),
        x,
        y,
        z,
    };
    download2(&download_id).await?;
    download_id.get_final_path()
}

// pub async fn load_geoduck_stats(
//     theme: &str,
//     _type: &str,
//     x: u64,
//     y: u64,
//     z: u8,
// ) -> Result<GeoDuckSegmentSummary> {
//     let download_id = OvertureMapsSegment {
//         theme: theme.to_string(),
//         _type: _type.to_string(),
//         x,
//         y,
//         z,
//     };
//     Ok(download2(&download_id).await?)
// }
