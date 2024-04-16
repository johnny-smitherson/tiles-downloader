use anyhow::Result;
use async_tempfile::TempFile;
use image::DynamicImage;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use image::io::Reader as ImageReader;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

use geojson::FeatureCollection;

use crate::config;
use crate::config::{
    ImageFetchDescriptor, TileServerConfig, DB_TILE_SERVER_CONFIGS, LINKS_CONFIG,
};
use rocket::tokio::task::spawn_blocking;

pub async fn validate_fetched_tile(
    img_path: &PathBuf,
    server_config: &TileServerConfig,
) -> Result<(String, DynamicImage)> {
    let img_path = img_path.clone();
    let server_config = server_config.clone();
    spawn_blocking(move|| {
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
    }).await?
}

pub async fn download_once(url: &str, path: &Path) -> Result<()> {
    let user_agent = LINKS_CONFIG
        .user_agents
        .choose(&mut rand::thread_rng())
        .context("no user-agent")?;
    let socks5_proxy = LINKS_CONFIG
        .socks5_proxy_list
        .choose(&mut rand::thread_rng())
        .context("no socks proxy")?;

    let mut curl_cmd = tokio::process::Command::new(LINKS_CONFIG.curl_path.clone());
    curl_cmd
        .arg("-s")
        .arg("-o")
        .arg(path)
        .arg("--user-agent")
        .arg(user_agent)
        .arg("--socks5-hostname")
        .arg(socks5_proxy)
        .arg("--connect-timeout")
        .arg(LINKS_CONFIG.timeout_secs.to_string())
        .arg("--max-time")
        .arg((LINKS_CONFIG.timeout_secs * 2).to_string())
        .arg(url);
    eprint!("running curl: {:?}\n", curl_cmd);
    let mut curl = curl_cmd.spawn()?;
    let curl_status = curl.wait().await?;
    if curl_status.success() {
        Ok(())
    } else {
        anyhow::bail!(
            "curl fail to get file \n using socks proxy = {:?} \n url = {:?}",
            socks5_proxy,
            url
        )
    }
}

async fn tempfile() -> Result<async_tempfile::TempFile> {
    let tmp_parent = LINKS_CONFIG.tile_location.join("tmp");
    tokio::fs::create_dir_all(&tmp_parent).await?;
    let temp_file = TempFile::new_in(tmp_parent).await?;
    Ok(temp_file)
}

async fn download_tile(
    server_config: &TileServerConfig,
    fetch_descriptor: &ImageFetchDescriptor,
) -> Result<PathBuf> {
    let temp_file = tempfile().await?;
    let final_file = fetch_descriptor.get_disk_path(&server_config).await?;

    // try a bunch of random settings
    for _ in 1..=LINKS_CONFIG.retries {
        let url = &fetch_descriptor.get_some_url(server_config)?;

        if download_once(url, temp_file.file_path()).await.is_ok() {
            validate_fetched_tile(temp_file.file_path(), server_config).await?;
            tokio::fs::rename(temp_file.file_path(), &final_file).await?;
            return Ok(final_file);
        }
    }

    anyhow::bail!("curl failed to get {:?}", fetch_descriptor);
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
        || crate::download::validate_fetched_tile(&path, &server_config)
            .await
            .is_err()
    {
        download_tile(&server_config, &fetch_info).await?;
    }
    Ok(path)
}

pub async fn is_json(path: &Path) -> Result<()> {
    let bytes = tokio::fs::read(path).await?;
    let _data: serde_json::Value = serde_json::from_slice(&bytes)?;
    Ok(())
}

pub async fn search_geojson_to_disk(query_str: &str) -> Result<std::path::PathBuf> {
    let query_urlencode = urlencoding::encode(query_str).into_owned();
    let dir_path = LINKS_CONFIG.tile_location.join("geojson");
    tokio::fs::create_dir_all(&dir_path).await?;
    let temp_file = tempfile().await?;
    let path = dir_path.join(format!("{}.geo.json", query_urlencode));
    let url = {
        let mut map: HashMap<String, String> = HashMap::with_capacity(10);
        map.insert("q_urlencoded".to_owned(), query_urlencode.clone());

        strfmt::strfmt(&LINKS_CONFIG.geo_search_url, &map)
            .context("failed strfmt on URL")?
    };

    if !path.exists() || is_json(&path).await.is_err() {
        download_once(&url, temp_file.file_path()).await?;
        // validate_geojson(&path).await?;
        tokio::fs::rename(temp_file.file_path(), &path).await?;
    }

    Ok(path)
}

pub async fn parse_geojson(path: &Path) -> Result<FeatureCollection> {
    let bytes = tokio::fs::read(path).await?;
    let data: FeatureCollection = serde_json::from_slice(&bytes)?;

    // eprintln!("{:#?}", data);
    Ok(data)
}

pub async fn search_geojson(query_str: &str) -> Result<FeatureCollection> {
    let path = search_geojson_to_disk(query_str).await?;
    Ok(parse_geojson(&path).await?)
}
