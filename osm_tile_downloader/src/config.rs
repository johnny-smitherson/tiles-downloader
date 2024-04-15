use std::io::Read;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LinksConfig {
    pub db_location: PathBuf,
    pub tile_location: PathBuf,
    pub user_agents: Vec<String>,
    pub fetch_rate: u8,
    pub timeout_secs: u64,
    pub retries: u8,
    pub servers: Vec<TileServerConfig>,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct TileServerConfig {
    pub name: String,
    pub comment: String,
    pub url: String,
    pub width: u16,
    pub height: u16,
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
    for tile_server in config.servers.iter() {
        assert!(
            tile_server.servers.is_none()
                || (tile_server.servers.is_some()
                    && !tile_server.servers.as_ref().unwrap().is_empty())
        );
    }

    Ok(config)
}
