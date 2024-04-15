#[macro_use]
extern crate rocket;

use std::path::PathBuf;

use anyhow::Context;
use config::TileServerConfig;
use rand::{self, seq::SliceRandom};
use reqwest::dns::Name;
use rocket::fs::NamedFile;
use rocket::response::status::NotFound;

mod rocket_anyhow;
pub(crate) use rocket_anyhow::bail;

pub(crate) mod config;

lazy_static::lazy_static! {
    pub static ref LINKS_CONFIG: config::LinksConfig = config::load_config().expect("bad config:");

    pub static ref SLED_DB: sled::Db = sled::open(LINKS_CONFIG.db_location.clone()).expect("cannot open db:");

    pub static ref DB_TILE_SERVER_CONFIGS: typed_sled::Tree::<String, config::TileServerConfig> = typed_sled::Tree::<String, config::TileServerConfig>::open(&*SLED_DB, "tile_server_configs");
}

#[get("/health_check")]
fn health_check() -> String {
    format!("ok. Config: {:#?}", *LINKS_CONFIG)
}

#[get("/favicon.ico")]
async fn favicon() -> Option<NamedFile> {
    NamedFile::open("./0.png").await.ok()
}

#[get("/info/server/<server>")]
fn info_server(server: String) -> String {
    format!("ok. server: {:#?}", server)
}

#[get("/info/zoom/<zoom>")]
fn info_zoom(zoom: u8) -> String {
    format!("ok. zoom: {:#?}", zoom)
}

#[get("/tile/<server_name>/<z>/<x>/<y>/<extension>")]
async fn tile_get_file(
    server_name: String,
    x: u64,
    y: u64,
    z: u8,
    extension: String,
) -> Result<NamedFile, rocket_anyhow::Error> {
    let server_config = DB_TILE_SERVER_CONFIGS
        .get(&server_name.to_owned())
        .context("db get error")?
        .with_context(|| format!("server_name not found: '{}'", &server_name))?;

    let fetch_info = ImageFetchDescriptor {
        x,
        y,
        z,
        server_name,
        extension,
    };
    fetch_info.validate(&server_config)?;

    let path = fetch_info.get_disk_path(&server_config).await?;

    Ok(
        NamedFile::open(&path).await
            .with_context(|| format!("file missing from disk: {:?}", &path))?
    )
}

struct ImageFetchDescriptor {
    x: u64,
    y: u64,
    z: u8,
    server_name: String,
    extension: String,
}

impl ImageFetchDescriptor {
    fn validate(
        self: &Self,
        server_config: &TileServerConfig,
    ) -> rocket_anyhow::Result<()> {
        if server_config.max_level < self.z {
            rocket_anyhow::bail!(
                "got z = {} when max for server is {}",
                self.z,
                server_config.max_level
            );
        };
        if self.z < 1 {
            rocket_anyhow::bail!("got z = {} when min for server is {}", self.z, 1);
        };
        if !(self.extension.eq(&server_config.img_type)) {
            rocket_anyhow::bail!(
                "got extension = {} when server img_type is {}",
                &self.extension,
                &server_config.img_type
            );
        };
        let max_extent = 2u64.pow(self.z.into()) - 1;
        if !(self.x <= max_extent && self.y <= max_extent) {
            rocket_anyhow::bail!(
                "x={}, y={} not inside extent={} for z={}",
                self.x,
                self.y,
                max_extent,
                self.z
            );
        }
        Ok(())
    }
    async fn get_disk_path(
        self: &ImageFetchDescriptor,
        server_config: &TileServerConfig,
    ) -> rocket_anyhow::Result<PathBuf> {
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
        target.push(format!("{}.{}", self.y.to_string(), self.extension));

        // let file = File::create(target).await?;
        Ok(target)
    }

    fn get_some_url(
        self: &ImageFetchDescriptor,
        server_config: &TileServerConfig,
    ) -> rocket_anyhow::Result<String> {
        use std::collections::HashMap;
        let mut map: HashMap<String, String> = HashMap::with_capacity(10);
        let server_bit = {
            if server_config.servers.is_none() {
                "".to_owned()
            } else {
                server_config
                    .servers
                    .as_ref().context("empty server letter")?
                    .choose(&mut rand::thread_rng())
                    .context("empty server vector")?
                    .to_owned()
            }
        };

        map.insert("s".to_owned(), server_bit);
        map.insert("x".to_owned(), self.x.to_string());
        map.insert("y".to_owned(), self.y.to_string());
        map.insert("z".to_owned(), self.z.to_string());

        Ok(strfmt::strfmt(&server_config.url, &map).context("failed strfmt on URL")?)
    }
}

#[rocket::main]
async fn main() -> rocket_anyhow::Result<()> {
    eprintln!(
        "ok. Config: {} ...",
        &format!("{:#?}", *LINKS_CONFIG).as_str()[..200]
    );
    for server_config in &mut *LINKS_CONFIG.servers.clone() {
        DB_TILE_SERVER_CONFIGS
            .insert(&server_config.name, &server_config)
            .context("cannot write to db:")?;
    }

    for db_tree_name in (*SLED_DB).tree_names().iter() {
        let tree = (*SLED_DB)
            .open_tree(&db_tree_name)
            .context("cannot open db tree: ")?;
        eprintln!(
            "found db tree {:?}: len = {}",
            String::from_utf8_lossy(db_tree_name),
            tree.len()
        )
    }

    let _rocket = rocket::build()
        .mount(
            "/",
            routes![health_check, favicon, info_server, info_zoom, tile_get_file],
        )
        .launch()
        .await?;

    Ok(())
}
