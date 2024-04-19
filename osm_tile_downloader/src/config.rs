use std::io::Read;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use anyhow::Context;
use rand::seq::SliceRandom;

lazy_static::lazy_static! {
    pub static ref LINKS_CONFIG: LinksConfig = load_config().expect("bad config:");

    pub static ref SLED_DB: sled::Db = sled::open(LINKS_CONFIG.db_location.clone()).expect("cannot open db:");

    pub static ref DB_TILE_SERVER_CONFIGS: 
        typed_sled::Tree::<String, TileServerConfig> 
        = typed_sled::Tree::<String, TileServerConfig>::open(&SLED_DB, "tile_server_configs");
    pub static ref DB_SOCKS_SCRAPER_CONFIGS: 
        typed_sled::Tree::<String, Socks5ProxyScraperConfig> 
        = typed_sled::Tree::<String, Socks5ProxyScraperConfig>::open(&SLED_DB, "socks5_scraper_configs");

    pub static ref DB_STAT_COUNTER:
        typed_sled::Tree::<String, StatCounter>
         = typed_sled::Tree::<String, StatCounter>::open(&SLED_DB, "stat_counter");
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Hash, Eq)]
pub struct StatCounterKey {
    _item_a: String,
    _item_b: String,
}
use std::collections::HashMap;
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StatCounter {
    kv: HashMap<StatCounterKey, u64>,
    edit_at: HashMap<StatCounterKey, f64>,
}

const STAT_COUNTER_ENTRY_TTL: f64 = 3600.0;

impl StatCounter {
    fn increment(&mut self, hash_key: &StatCounterKey) {
        self.kv
            .insert(hash_key.clone(), self.kv.get(&hash_key).unwrap_or(&0) + 1);
        self.edit_at
            .insert(hash_key.clone(), get_current_timestamp());

        let mut keys_to_delete = vec![];
        let delete_before = get_current_timestamp() - STAT_COUNTER_ENTRY_TTL;
        for (k, v) in self.edit_at.iter() {
            if *v < delete_before {
                keys_to_delete.push(k.clone());
            }
        }
        for k in keys_to_delete {
            self.kv.remove(&k);
            self.edit_at.remove(&k);
        }
    }
}

pub fn get_current_timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs_f64()
}

pub fn stat_counter_increment(
    stat_type: &str,
    stat_event: &str,
    stat_item_a: &str,
    stat_item_b: &str,
) -> anyhow::Result<()> {
    let hash_key = StatCounterKey {
        _item_a: stat_item_a.to_owned(),
        _item_b: stat_item_b.to_owned(),
    };
    let db_key = format!("{}-{}", stat_type, stat_event);

    DB_STAT_COUNTER.update_and_fetch(&db_key, |v| match v {
        Some(mut stat_counter) => {
            stat_counter.increment(&hash_key);    
            Some(stat_counter)
        },
        None => {
            let mut stat_counter = StatCounter {
                kv: HashMap::new(),
                edit_at: HashMap::new(),
            };
            stat_counter.increment(&hash_key);
            Some(stat_counter)
        }
    })?;
    Ok(())
}

pub fn stat_counter_get_all() -> Vec<(String, String, String, u64)> {
    let mut _vec = vec![];

    DB_STAT_COUNTER.iter().for_each(|x| {
        if let Ok((db_key, v)) = x {
            for (hash_key, value) in v.kv.iter() {
                _vec.push((db_key.clone(), hash_key._item_a.to_owned(), hash_key._item_b.to_owned(), *value));
            }
        }
    });
    _vec.sort();
    _vec
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LinksConfig {
    pub db_location: PathBuf,
    pub tile_location: PathBuf,
    pub user_agents: Vec<String>,
    pub tor_addr_list: Vec<String>,
    pub fetch_rate: u8,
    pub timeout_secs: u64,
    pub retries: u8,
    pub curl_path: PathBuf,
    pub tile_servers: Vec<TileServerConfig>,
    pub socks5_scrape_servers: Vec<Socks5ProxyScraperConfig>,
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
    assert!(config.tile_servers.len() > 0);
    assert!(config.user_agents.len() > 0);
    assert!(config.tor_addr_list.len() > 0);

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
        map.insert(
            "bing_quadkey".to_owned(),
            crate::geo_trig::xyz_to_bing_quadkey(self.x, self.y, self.z),
        );

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

pub fn get_all_socks5_scrapers() -> anyhow::Result<Vec<Socks5ProxyScraperConfig>> {
    let mut servers = Vec::<Socks5ProxyScraperConfig>::new();
    for k in DB_SOCKS_SCRAPER_CONFIGS.iter() {
        let (_, value) = k?;
        servers.push(value);
    }
    Ok(servers)
}

pub async fn tempfile() -> anyhow::Result<async_tempfile::TempFile> {
    let tmp_parent = LINKS_CONFIG.tile_location.join("tmp");
    tokio::fs::create_dir_all(&tmp_parent).await?;
    let temp_file = async_tempfile::TempFile::new_in(tmp_parent).await?;
    Ok(temp_file)
}
