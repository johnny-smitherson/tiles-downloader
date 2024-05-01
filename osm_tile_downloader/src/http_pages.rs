use anyhow::anyhow;
use rocket::fs::NamedFile;
use rocket_dyn_templates::context;
use rocket_dyn_templates::Template;

use crate::config;
use crate::config::TileServerConfig;
use crate::config::LINKS_CONFIG;
use crate::download_everything;
use crate::download_geosearch;
use crate::download_tile::OverlayDrawCoordinates;
use crate::geo_trig;
use crate::http_api;
use crate::proxy_manager;
use crate::rocket_anyhow;

pub fn get_page_routes() -> Vec<rocket::Route> {
    routes![index, health_check, favicon, geo_index, proxy_info,]
}

#[get("/health_check")]
fn health_check() -> String {
    format!("ok. Config: {:#?}", *LINKS_CONFIG)
}

#[get("/favicon.ico")]
async fn favicon() -> Option<NamedFile> {
    NamedFile::open("./0.png").await.ok()
}

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
            all_working_proxies: proxy_manager::get_all_working_proxies(),
            all_broken_proxies: proxy_manager::get_all_broken_proxies(),
            stat_counters: config::stat_counter_get_all(),
        },
    ))
}

#[get("/geo/<q_location>")]
async fn geo_index(q_location: &str) -> rocket_anyhow::Result<Template> {
    let geo_search_results =
        download_geosearch::search_geojson(q_location).await?;
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
                        feature.geo_point.x_lon,
                        feature.geo_point.y_lat,
                    );
                    let server_name = srv.name.clone();
                    let ext = srv.img_type.clone();
                    let overlay =
                        crate::download_tile::OverlayDrawCoordinates {
                            point: Some(feature.geo_point),
                            bbox: Some(feature.bbox),
                        };
                    let the_uri = uri!(http_api::get_tile_with_overlay(
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
            feature: feature.clone(),
            feature_point: format!("{:?}", feature.geo_point),
            feature_bbox: format!("{:?}", feature.bbox),
            q_location: q_location,
            txt_download_everything: format!("{:#?}", download_everything::download_everything(&feature.geo_point).await?)
        },
    ))
}
