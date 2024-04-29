use std::os::windows::fs::MetadataExt;

use crate::config;
use crate::download_geoduck;
use crate::download_tile::get_tile;
use crate::geo_trig;
use crate::geo_trig::GeoPoint;
use crate::http_api;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct DownloadEverythingSummary {
    pub success_count: u64,
    pub error_count: u64,
    pub total_size_mb: f64,
    pub v_tiles: Vec<DownloadEverythingItem>,
    pub v_geoducks: Vec<DownloadEverythingItem>,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct DownloadEverythingItem {
    pub name: String,
    pub url: String,
    pub result: String,
    pub success: bool,
    pub file_size_mb: f64,
    pub item_theme: String,
    pub item_type: String,
    pub x: u64,
    pub y: u64,
    pub z: u8,
}
async fn download_all_tiles(
    point: &GeoPoint,
) -> anyhow::Result<Vec<DownloadEverythingItem>> {
    let mut v = vec![];

    let tile_servers = config::get_all_tile_servers()?;
    for srv in tile_servers.iter() {
        for z in 1..=srv.max_level {
            let (x, y) = geo_trig::tile_index(z, point.x_lon, point.y_lat);
            let server_name = srv.name.clone();
            let ext = srv.img_type.clone();
            let url = uri!(http_api::get_tile(
                server_name = server_name.clone(),
                x = x,
                y = y,
                z = z,
                extension = ext.clone(),
            ))
            .path()
            .to_string();

            let rv = get_tile(&server_name, x, y, z, &ext).await;
            let file_size_mb = if let Ok(p) = &rv {
                tokio::fs::metadata(p).await?.file_size() as f64 / 1024.0 / 1024.0
            } else {
                0.0
            };
            let item_name = format!(
                "tile {}/{} z={} x={} y={}",
                &srv.map_type, server_name, z, x, y
            );
            v.push(DownloadEverythingItem {
                name: item_name,
                url,
                result: format!("{:?}", &rv),
                success: rv.is_ok(),
                file_size_mb,
                item_theme: srv.map_type.clone(),
                item_type: server_name.clone(),
                x,
                y,
                z,
            });
        }
    }

    Ok(v)
}

async fn download_all_geoduck(
    point: &GeoPoint,
) -> anyhow::Result<Vec<DownloadEverythingItem>> {
    let mut v = vec![];

    for z in download_geoduck::GEODUCK_ZOOM_LEVEL
        ..=download_geoduck::PARQUET_MAX_ZOOM_LEVEL
    {
        let (x, y) = geo_trig::tile_index(z, point.x_lon, point.y_lat);
        for (theme, _type) in download_geoduck::OVERT_THEMES.iter() {
            let item_name =
                format!("geoduck {}/{} z={} x={} y={}", theme, _type, z, x, y);
            let rv =
                download_geoduck::download_geoduck_to_disk(theme, _type, x, y, z)
                    .await;
            let url = rocket::uri!(http_api::get_overt_geoduck(
                theme = theme,
                o_type = _type,
                x = x,
                y = y,
                z = z
            ))
            .path()
            .to_string();
            let file_size_mb = if let Ok(p) = &rv {
                tokio::fs::metadata(p).await?.file_size() as f64 / 1024.0 / 1024.0
            } else {
                0.0
            };
            v.push(DownloadEverythingItem {
                name: item_name,
                url,
                result: format!("{:?}", &rv),
                success: rv.is_ok(),
                file_size_mb,
                item_theme: theme.clone(),
                item_type: _type.clone(),
                x,
                y,
                z,
            });
        }
    }

    Ok(v)
}
pub async fn download_everything(
    point: &GeoPoint,
) -> anyhow::Result<DownloadEverythingSummary> {
    let v_tiles = download_all_tiles(point).await?;
    let v_geoducks = download_all_geoduck(point).await?;
    let fail_count = v_geoducks.iter().filter(|x| !x.success).count()
        + v_tiles.iter().filter(|x| !x.success).count();
    let success_count = v_geoducks.iter().filter(|x| x.success).count()
        + v_tiles.iter().filter(|x| x.success).count();
    let total_size_mb: f64 = v_tiles.iter().map(|x| x.file_size_mb).sum::<f64>()
        + v_geoducks.iter().map(|x| x.file_size_mb).sum::<f64>();
    Ok(DownloadEverythingSummary {
        success_count: success_count as u64,
        error_count: fail_count as u64,
        total_size_mb,
        v_geoducks,
        v_tiles,
    })
}
