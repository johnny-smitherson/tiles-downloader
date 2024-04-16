use std::io::Read;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use anyhow::Context;
use rand::seq::SliceRandom;

lazy_static::lazy_static! {
    pub static ref LINKS_CONFIG: LinksConfig = load_config().expect("bad config:");

    pub static ref SLED_DB: sled::Db = sled::open(LINKS_CONFIG.db_location.clone()).expect("cannot open db:");

    pub static ref DB_TILE_SERVER_CONFIGS: typed_sled::Tree::<String, TileServerConfig> = typed_sled::Tree::<String, TileServerConfig>::open(&SLED_DB, "tile_server_configs");
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LinksConfig {
    pub db_location: PathBuf,
    pub tile_location: PathBuf,
    pub user_agents: Vec<String>,
    pub socks5_proxy_list: Vec<String>,
    pub fetch_rate: u8,
    pub timeout_secs: u64,
    pub retries: u8,
    pub curl_path: PathBuf,
    pub servers: Vec<TileServerConfig>,
    pub geo_search_url: String,
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
    assert!(config.servers.len() > 0);
    assert!(config.user_agents.len() > 0);
    assert!(config.socks5_proxy_list.len() > 0);

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
        config.clone().servers.iter().map(|x| x.name.clone())
    ));

    // CHECK TILE SERVER CONFIGS
    for tile_server in config.servers.iter() {
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
    for server_config in &mut *LINKS_CONFIG.servers.clone() {
        DB_TILE_SERVER_CONFIGS
            .insert(&server_config.name, server_config)
            .context("cannot write to db:")?;
    }

    for db_tree_name in (*SLED_DB).tree_names().iter() {
        let tree = (*SLED_DB)
            .open_tree(db_tree_name)
            .context("cannot open db tree: ")?;
        eprintln!(
            "found db tree {:?}: len = {}",
            String::from_utf8_lossy(db_tree_name),
            tree.len()
        )
    }
    Ok(())
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct ImageFetchDescriptor {
    pub x: u64,
    pub y: u64,
    pub z: u8,
    pub server_name: String,
    pub extension: String,
}

impl ImageFetchDescriptor {
    pub fn validate(&self, server_config: &TileServerConfig) -> anyhow::Result<()> {
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
    pub async fn get_disk_path(
        self: &ImageFetchDescriptor,
        server_config: &TileServerConfig,
    ) -> anyhow::Result<PathBuf> {
        assert!(server_config.name.eq(&self.server_name));
        assert!(server_config.img_type.eq(&self.extension));
        let mut target = LINKS_CONFIG
            .tile_location
            .clone()
            .join(&server_config.map_type)
            .join(&server_config.name)
            .join(self.z.to_string())
            .join(self.x.to_string());
        tokio::fs::create_dir_all(&target).await.with_context(|| {
            format!(
                "failed creating output directory for tile {}x{}x{}",
                self.x, self.y, self.z
            )
        })?;
        target.push(format!("{}.{}", self.y, self.extension));

        // let file = File::create(target).await?;
        Ok(target)
    }

    pub fn get_some_url(
        self: &ImageFetchDescriptor,
        server_config: &TileServerConfig,
    ) -> anyhow::Result<String> {
        use std::collections::HashMap;
        let mut map: HashMap<String, String> = HashMap::with_capacity(10);
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

        Ok(strfmt::strfmt(&server_config.url, &map)
            .context("failed strfmt on URL")?)
    }
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
        .with_context(|| format!("server_name not found: '{}'", &server_name))?;
    Ok(server_config)
}
