pub(crate) mod config;
pub(crate) mod download_tile;
pub(crate) mod fetch;
pub(crate) mod geo_trig;
pub(crate) mod overt_geo_duck;
pub(crate) mod proxy_manager;
pub(crate) mod rocket_anyhow;

#[macro_use]
extern crate rocket;

use anyhow::anyhow;
use anyhow::Context;
use rocket::fs::NamedFile;
use rocket_dyn_templates::context;
use rocket_dyn_templates::Template;

use config::init_database;
use config::TileServerConfig;
use config::LINKS_CONFIG;

#[get("/")]
fn index() -> rocket_anyhow::Result<Template> {
    Ok(Template::render(
        "index",
        context! {
            tile_servers: config::get_all_tile_servers()?,
        },
    ))
}

#[get("/proxy")]
async fn proxy_info() -> rocket_anyhow::Result<Template> {
    let scrapers = config::get_all_socks5_scrapers()?;
    let scraper_events = config::stat_count_events_for_items(
        &scrapers.iter().map(|e| e.name.as_str()).collect(),
    );
    let scrapers: Vec<(_, Vec<_>)> = scrapers
        .iter()
        .map(|s| (s, scraper_events.get(&s.name).unwrap().iter().collect()))
        .collect();

    Ok(Template::render(
        "proxy",
        context! {
            // fetch_queue_ready: crate::fetch::fetch_queue_ready()?,
            // fetch_queue_done: crate::fetch::fetch_queue_done()?,
            scrapers: scrapers,
            all_working_proxies: crate::proxy_manager::get_all_working_proxies(),
            all_broken_proxies: crate::proxy_manager::get_all_broken_proxies(),
            stat_counters: config::stat_counter_get_all(),
        },
    ))
}

use rocket::form::Form;

#[derive(FromForm)]
struct GeoDuckReplRequest {
    sql_query: String,
}

#[post("/api/geoduck/repl", data = "<form>")]
async fn geoduck_repl_api(
    form: Form<GeoDuckReplRequest>,
) -> rocket_anyhow::Result<String> {
    Ok(crate::overt_geo_duck::geoduck_execute_to_str(&form.sql_query).await?)
}

#[get("/geoduck/repl")]
fn geoduck_repl() -> rocket_anyhow::Result<Template> {
    Ok(Template::render("geoduck", context! {}))
}

#[get("/health_check")]
fn health_check() -> String {
    format!("ok. Config: {:#?}", *LINKS_CONFIG)
}

#[get("/favicon.ico")]
async fn favicon() -> Option<NamedFile> {
    NamedFile::open("./0.png").await.ok()
}

#[get("/api/geo/<q_location>/json")]
async fn geo_search_json(q_location: &str) -> rocket_anyhow::Result<NamedFile> {
    let geojson_path =
        crate::download_tile::search_geojson_to_disk(q_location).await?;

    Ok(NamedFile::open(&geojson_path)
        .await
        .with_context(|| format!("file missing from disk: {:?}", &geojson_path))?)
}

#[get("/geo/<q_location>")]
async fn geo_index(q_location: &str) -> rocket_anyhow::Result<Template> {
    let geo_search_results = download_tile::search_geojson(q_location).await?;
    if geo_search_results.is_empty() {
        return Err(anyhow!(
            "no features found after searching for '{}'",
            q_location
        )
        .into());
    }
    let feature = geo_search_results[0].clone();

    let tile_servers = config::get_all_tile_servers()?;
    #[derive(serde::Serialize)]
    struct TileServerEntry {
        srv: TileServerConfig,
        links: Vec<(u8, u64, u64, String)>,
    }
    let tile_servers: Vec<_> = tile_servers
        .iter()
        .map(|srv| TileServerEntry {
            srv: srv.clone(),
            links: (1..=srv.max_level)
                .map(|z| {
                    let (x, y) = geo_trig::tile_index(
                        z,
                        feature.geo_point.point_0,
                        feature.geo_point.point_1,
                    );
                    let server_name = srv.name.clone();
                    let ext = srv.img_type.clone();
                    let overlay = crate::download_tile::OverlayDrawCoordinates {
                        point: Some(feature.geo_point.clone()),
                        bbox: Some(feature.bbox.clone()),
                    };
                    let the_uri = uri!(get_tile_with_overlay(
                        server_name = server_name,
                        x = x,
                        y = y,
                        z = z,
                        extension = ext,
                        overlay_coordinates = overlay
                    ));
                    (
                        z,
                        x,
                        y,
                        format!(
                            "{}?{}",
                            the_uri.path().as_str(),
                            the_uri.query().unwrap().as_str()
                        ),
                    )
                })
                .collect(),
        })
        .collect();
    Ok(Template::render(
        "geo_location",
        context! {
            tileserver_with_links: tile_servers,
            geo_point: feature.geo_point,
            display_name: feature.display_name,
            q_location: q_location,
        },
    ))
}

#[get("/api/tile/<server_name>/<z>/<x>/<y>/<extension>")]
async fn get_tile(
    server_name: &str,
    x: u64,
    y: u64,
    z: u8,
    extension: &str,
) -> rocket_anyhow::Result<Option<NamedFile>> {
    let extension = extension.to_owned();
    let extension = if extension.contains('.') {
        extension.split('.').last().context("??")?
    } else {
        extension.as_str()
    };
    if !extension.eq("png") && !extension.eq("jpg") {
        return Ok(None);
    }
    let path =
        crate::download_tile::get_tile(server_name, x, y, z, extension).await?;

    Ok(Some(NamedFile::open(&path).await.with_context(|| {
        format!("file missing from disk: {:?}", &path)
    })?))
}

use rocket::http::ContentType;
use rocket::response::Responder;
use rocket::Response;
use std::io::Cursor;
use tokio::task::spawn_blocking;

pub struct ImageResponse {
    img_bytes: Vec<u8>,
    content_type: ContentType,
}

#[rocket::async_trait]
impl<'r> Responder<'r, 'static> for ImageResponse {
    fn respond_to(
        self,
        _: &'r rocket::Request<'_>,
    ) -> rocket::response::Result<'static> {
        Response::build()
            .header(self.content_type)
            .sized_body(self.img_bytes.len(), Cursor::new(self.img_bytes))
            .ok()
    }
}

#[get("/api/tile_with_overlay/<server_name>/<z>/<x>/<y>/<extension>?<overlay_coordinates..>")]
async fn get_tile_with_overlay(
    server_name: &str,
    x: u64,
    y: u64,
    z: u8,
    extension: &str,
    overlay_coordinates: crate::download_tile::OverlayDrawCoordinates,
) -> rocket_anyhow::Result<ImageResponse> {
    let path =
        crate::download_tile::get_tile(server_name, x, y, z, extension).await?;
    let server_config = config::get_tile_server(server_name)?;
    let img_type = server_config.img_type.clone();
    assert!(img_type.eq(extension));

    let content_type =
        ContentType::from_extension(extension).context("bad extension?")?;

    let img_bytes = download_tile::draw_overlay_on_tile(
        x,
        y,
        z,
        extension,
        &path,
        &overlay_coordinates,
        &server_config,
    )
    .await?;

    let img_response = ImageResponse {
        img_bytes,
        content_type,
    };
    Ok(img_response)
}

#[rocket::main]
async fn main() -> rocket_anyhow::Result<()> {
    init_database().await?;
    overt_geo_duck::init_geoduck()?;
    // check we can run the manager once
    // let _fetch_manager = tokio::spawn(fetch::fetch_loop());
    let _proxy_manager = tokio::spawn(proxy_manager::proxy_manager_loop());

    let _rocket = rocket::build()
        .mount(
            "/",
            routes![
                index,
                health_check,
                favicon,
                get_tile,
                get_tile_with_overlay,
                geo_search_json,
                geo_index,
                proxy_info,
                geoduck_repl,
                geoduck_repl_api,
            ],
        )
        .attach(Template::fairing())
        .launch()
        .await?;

    eprintln!("aborting worker loops...");
    _proxy_manager.abort();
    // _fetch_manager.abort();
    eprintln!("clean exit done.");

    Ok(())
}
