use std::io::Read;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use anyhow::Context;

lazy_static::lazy_static! {
    pub static ref LINKS_CONFIG: LinksConfig = load_config().expect("bad config:");

    pub static ref SLED_DB: sled::Db = sled::open(
        LINKS_CONFIG.db_location.clone()
    ).expect("cannot open db:");

    pub static ref DB_TILE_SERVER_CONFIGS:
        typed_sled::Tree::<String, TileServerConfig>
        = typed_sled::Tree::<String, TileServerConfig>::open(
            &SLED_DB,
            "tile_server_configs_v3");

    pub static ref DB_SOCKS_SCRAPER_CONFIGS:
        typed_sled::Tree::<String, Socks5ProxyScraperConfig>
        = typed_sled::Tree::<String, Socks5ProxyScraperConfig>::open(
            &SLED_DB,
            "socks5_scraper_configs_v2");

}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LinksConfig {
    pub db_location: PathBuf,
    pub tile_location: PathBuf,
    pub user_agents: Vec<String>,
    pub tor_addr_list: Vec<String>,
    pub proxy_fetch_parallel: u8,
    pub timeout_secs: u64,
    pub retries: u8,
    pub curl_path: PathBuf,
    pub curl_impersonate_path: PathBuf,
    pub tile_servers: Vec<TileServerConfig>,
    pub socks5_scrape_servers: Vec<Socks5ProxyScraperConfig>,
    pub geo_search_url: String,
    pub topography_servers: Vec<TopographyServerConfig>,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct TopographyServerConfig {
    pub name: String,
    pub comment: String,
    pub url: String,
    pub download_zoomlevel: u32,
    pub scale_zoomlevel: u32,
}


#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct TileServerConfig {
    pub name: String,
    pub comment: String,
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub max_level: u8,
    pub img_type: String,
    pub map_type: String,
    pub servers: Option<Vec<String>>,
    pub planet: String,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct Socks5ProxyScraperConfig {
    pub name: String,
    pub url: String,
    pub extract_method: String,
}

pub fn load_config() -> anyhow::Result<LinksConfig> {
    let links_config_path = "./config/links.toml";

    let mut buf = String::new();
    std::fs::File::open(links_config_path)
        .unwrap()
        .read_to_string(&mut buf)
        .expect("cannot read config: ");
    let config: LinksConfig = toml::from_str(buf.as_str())?;

    // CHECK LISTS ARE OK
    assert!(!config.tile_servers.is_empty());
    assert!(!config.user_agents.is_empty());
    assert!(!config.tor_addr_list.is_empty());

    // CHECK UNIQUE ELEMENTS IN TILE SERVER LIST
    fn has_unique_elements<T>(iter: T) -> bool
    where
        T: IntoIterator,
        T::Item: Eq + std::hash::Hash,
    {
        let mut uniq = std::collections::HashSet::new();
        iter.into_iter().all(move |x| uniq.insert(x))
    }
    assert!(has_unique_elements(
        config.clone().tile_servers.iter().map(|x| x.name.clone())
    ));
    assert!(has_unique_elements(
        config
            .clone()
            .socks5_scrape_servers
            .iter()
            .map(|x| x.name.clone())
    ));

    // CHECK TILE SERVER CONFIGS
    for tile_server in config.tile_servers.iter() {
        assert!(
            tile_server.servers.is_none()
                || (tile_server.servers.is_some()
                    && !tile_server.servers.as_ref().unwrap().is_empty())
        );
    }

    Ok(config)
}

pub async fn init_database() -> anyhow::Result<()> {
    eprintln!(
        "ok. Config: {} ...",
        &format!("{:#?}", *LINKS_CONFIG).as_str()[..200]
    );
    clear_tempfiles().await?;
    for server_config in &mut *LINKS_CONFIG.tile_servers.clone() {
        DB_TILE_SERVER_CONFIGS
            .insert(&server_config.name, server_config)
            .context("cannot write to db:")?;
    }

    for server_config in &mut *LINKS_CONFIG.socks5_scrape_servers.clone() {
        DB_SOCKS_SCRAPER_CONFIGS
            .insert(&server_config.name, server_config)
            .context("cannot write db:")?;
    }

    for db_tree_name in (*SLED_DB).tree_names().iter() {
        let mut total_size = 0;
        let tree = (*SLED_DB)
            .open_tree(db_tree_name)
            .context("cannot open db tree: ")?;
        for k in tree.iter() {
            let (key, val) = k?;
            total_size += key.len() + val.len();
        }
        eprintln!(
            "found db tree {:?}: len = {} ; size = {} KB",
            String::from_utf8_lossy(db_tree_name),
            tree.len(),
            total_size / 1024,
        )
    }
    Ok(())
}

pub fn get_all_tile_servers() -> anyhow::Result<Vec<TileServerConfig>> {
    let mut tile_servers = Vec::<TileServerConfig>::new();
    for k in DB_TILE_SERVER_CONFIGS.iter() {
        let (_, value) = k?;
        tile_servers.push(value);
    }
    Ok(tile_servers)
}

pub fn get_tile_server(server_name: &str) -> anyhow::Result<TileServerConfig> {
    let server_config = DB_TILE_SERVER_CONFIGS
        .get(&server_name.to_owned())
        .context("db get error")?
        .with_context(|| {
            format!("server_name not found: '{}'", &server_name)
        })?;
    Ok(server_config)
}

pub fn get_all_socks5_scrapers() -> anyhow::Result<Vec<Socks5ProxyScraperConfig>>
{
    let mut servers = Vec::<Socks5ProxyScraperConfig>::new();
    for k in DB_SOCKS_SCRAPER_CONFIGS.iter() {
        let (_, value) = k?;
        servers.push(value);
    }
    Ok(servers)
}

pub fn tmpdir() -> PathBuf {
    LINKS_CONFIG.tile_location.join("tmp")
}

pub async fn tempfile(name: &str) -> anyhow::Result<async_tempfile::TempFile> {
    let tmp_parent = tmpdir();
    tokio::fs::create_dir_all(&tmp_parent).await?;
    use rand::Rng;
    let rand_name =
        format!("tmp.{}.{}", rand::thread_rng().gen::<u128>(), name);
    let temp_file =
        async_tempfile::TempFile::new_with_name_in(rand_name, tmp_parent)
            .await?;
    Ok(temp_file)
}

pub async fn clear_tempfiles() -> anyhow::Result<()> {
    let tmp_parent = tmpdir();
    tokio::fs::remove_dir_all(&tmp_parent).await?;
    tokio::fs::create_dir_all(&tmp_parent).await?;
    tokio::fs::remove_dir_all(".tmp").await?;
    tokio::fs::create_dir_all(".tmp").await?;
    Ok(())
}

pub fn get_current_timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs_f64()
}
