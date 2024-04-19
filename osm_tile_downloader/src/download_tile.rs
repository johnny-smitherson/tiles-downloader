use anyhow::Result;
use futures::FutureExt;
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
use crate::config::{
    ImageFetchDescriptor, TileServerConfig, DB_TILE_SERVER_CONFIGS, LINKS_CONFIG,
};
use crate::proxy_manager;

pub fn validate_fetched_tile(
    img_path: &PathBuf,
    server_config: &TileServerConfig,
) -> Result<(String, DynamicImage)> {
    let img_path = img_path.clone();
    let server_config = server_config.clone();
    // let bytes = tokio::fs::read(img_path).await?;
    let bytes = std::fs::read(img_path)?;
    let img_reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let img_format = img_reader
        .format()
        .context("auto-guesser failed to get img format")?;
    let found_extension = img_format
        .extensions_str()
        .iter()
        .find(|&&x| x.to_owned() == server_config.img_type)
        .is_some();
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
    Ok((server_config.img_type.clone(), img))
}

async fn download_tile(
    server_config: &TileServerConfig,
    fetch_descriptor: &ImageFetchDescriptor,
) -> Result<PathBuf> {
    let final_file = fetch_descriptor.get_disk_path(&server_config).await?;
    let url = &fetch_descriptor.get_some_url(server_config)?;

    let _server_config_2 = server_config.clone();
    proxy_manager::download(url, &final_file, move |path| {
        validate_fetched_tile(path, &_server_config_2.clone())?;
        Ok(())
    })
    .await?;
    Ok(final_file)
}

pub async fn get_tile(
    server_name: &str,
    x: u64,
    y: u64,
    z: u8,
    extension: &str,
) -> Result<PathBuf> {
    let server_config = config::get_tile_server(server_name)?;

    let fetch_info = ImageFetchDescriptor {
        x,
        y,
        z,
        server_name: server_name.to_owned(),
        extension: extension.to_owned(),
    };
    fetch_info.validate(&server_config)?;

    let path = fetch_info.get_disk_path(&server_config).await?;

    if (!path.exists())
        || crate::download_tile::validate_fetched_tile(&path, &server_config)
            .is_err()
    {
        download_tile(&server_config, &fetch_info).await?;
    }
    Ok(path)
}

pub fn is_json(path: &Path) -> Result<()> {
    let bytes = std::fs::read(path)?;
    let _data: serde_json::Value = serde_json::from_slice(&bytes)?;
    Ok(())
}

pub async fn search_geojson_to_disk(query_str: &str) -> Result<std::path::PathBuf> {
    let query_urlencode = urlencoding::encode(query_str).into_owned();
    let dir_path = LINKS_CONFIG.tile_location.join("geojson");
    tokio::fs::create_dir_all(&dir_path).await?;
    let path = dir_path.join(format!("{}.geo.json", query_urlencode));
    let url = {
        let mut map: HashMap<String, String> = HashMap::with_capacity(10);
        map.insert("q_urlencoded".to_owned(), query_urlencode.clone());

        strfmt::strfmt(&LINKS_CONFIG.geo_search_url, &map)
            .context("failed strfmt on URL")?
    };

    if !path.exists() || is_json(&path).is_err() {
        crate::proxy_manager::download(&url, &path, |path| is_json(&path)).await?;
    }

    Ok(path)
}

pub async fn parse_geojson(path: &Path) -> Result<FeatureCollection> {
    let bytes = tokio::fs::read(path).await?;
    let data: FeatureCollection = serde_json::from_slice(&bytes)?;
    Ok(data)
}

pub async fn search_geojson(query_str: &str) -> Result<FeatureCollection> {
    let path = search_geojson_to_disk(query_str).await?;
    Ok(parse_geojson(&path).await?)
}
