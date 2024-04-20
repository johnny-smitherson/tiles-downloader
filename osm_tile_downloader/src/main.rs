pub(crate) mod config;
pub(crate) mod fetch;
pub(crate) mod download_tile;
pub(crate) mod geo_trig;
pub(crate) mod proxy_manager;
pub(crate) mod rocket_anyhow;

#[macro_use]
extern crate rocket;

use anyhow::anyhow;
use anyhow::Context;
use config::init_database;
use config::TileServerConfig;
use fetch::fetch;
use geojson::Bbox;
use image::DynamicImage;
use rocket::form::Form;
use rocket::fs::NamedFile;

use config::{ImageFetchDescriptor, DB_TILE_SERVER_CONFIGS, LINKS_CONFIG};
use rocket_dyn_templates::context;
use rocket_dyn_templates::Template;

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
    let scraper_events = config::stat_count_events_for_items(&scrapers.iter().map(|e| e.name.as_str()).collect());
    let scrapers: Vec<(_, Vec<_>)> = scrapers.iter().map(|s| (s, scraper_events.get(&s.name).unwrap().iter().collect())).collect();

    Ok(Template::render(
        "proxy",
        context! {
            fetch_queue_ready: crate::fetch::fetch_queue_ready()?,
            fetch_queue_done: crate::fetch::fetch_queue_done()?,
            scrapers: scrapers,
            all_working_proxies: crate::proxy_manager::get_all_working_proxies(),
            all_broken_proxies: crate::proxy_manager::get_all_broken_proxies(),
            stat_counters: config::stat_counter_get_all(),
        },
    ))
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
    let geo_collection = download_tile::search_geojson(q_location).await?;
    if geo_collection.features.is_empty() {
        return Err(anyhow!(
            "no features found after searching for '{}'",
            q_location
        )
        .into());
    }
    let geo_point = &geo_collection.features[0]
        .geometry
        .clone()
        .context("no geometry?")?
        .value;
    let geo_point = {
        if let geojson::Value::Point(coords) = geo_point {
            (coords[0], coords[1])
        } else {
            return Err(anyhow!("geometry was not point - ").into());
        }
    };
    let bbox = &geo_collection.features[0].bbox.clone();
    let display_name = geo_collection.features[0]
        .properties
        .clone()
        .context("no properties")?
        .get("display_name")
        .context("no display name?")?
        .clone();
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
                    let (x, y) = geo_trig::tile_index(z, geo_point.0, geo_point.1);
                    let server_name = srv.name.clone();
                    let ext = srv.img_type.clone();
                    let overlay = OverlayDrawCoordinates {
                        point: Some(OverlayDrawPoint {
                            point_0: geo_point.0,
                            point_1: geo_point.1,
                        }),
                        bbox: if bbox.is_none() {
                            None
                        } else {
                            let bbox = bbox.clone().unwrap();
                            Some(OverlayDrawBox {
                                bbox_0: bbox[0],
                                bbox_1: bbox[1],
                                bbox_2: bbox[2],
                                bbox_3: bbox[3],
                            })
                        },
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
            geo_point: geo_point,
            display_name: display_name,
            q_location: q_location,
            geo_collection: geo_collection,
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
    let extension = if extension.contains(".") {
        extension.split(".").last().context("??")?
    } else {
        extension.as_str()
    };
    if !extension.eq("png") && !extension.eq("jpg") {
        return Ok(None);
    }
    let (path, _) =
        crate::download_tile::get_tile(server_name, x, y, z, extension).await?;

    Ok(Some(NamedFile::open(&path).await.with_context(|| {
        format!("file missing from disk: {:?}", &path)
    })?))
}

#[derive(FromForm, UriDisplayQuery)]
struct OverlayDrawPoint {
    point_0: f64,
    point_1: f64,
}

#[derive(FromForm, UriDisplayQuery)]
struct OverlayDrawBox {
    bbox_0: f64,
    bbox_1: f64,
    bbox_2: f64,
    bbox_3: f64,
}

#[derive(FromForm, UriDisplayQuery)]
struct OverlayDrawCoordinates {
    point: Option<OverlayDrawPoint>,
    bbox: Option<OverlayDrawBox>,
}
use rocket::http::ContentType;
use rocket::http::Status;
use rocket::response::Responder;
use rocket::Response;
use std::f64::consts::PI;
use std::future::IntoFuture;
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
    overlay_coordinates: OverlayDrawCoordinates,
) -> rocket_anyhow::Result<ImageResponse> {
    let (_path, img) =
        crate::download_tile::get_tile(server_name, x, y, z, extension).await?;
    let server_config = config::get_tile_server(server_name)?;
    let _server_config_for_clz = server_config.clone();
    let img_type = server_config.img_type.clone();

    assert!(img_type.eq(extension));
    let content_type =
        ContentType::from_extension(extension).context("bad extension?")?;
    let image_format = match extension {
        "png" => image::ImageFormat::Png,
        "jpg" => image::ImageFormat::Jpeg,
        _ => rocket_anyhow::bail!("bad format: {}", extension),
    };

    let b_px = overlay_coordinates.point.context("no point coord!")?;
    let b_px = geo_trig::tile_index_float(z, b_px.point_0, b_px.point_1);

    let tile2pixel = |point: (f64, f64)| {
        (
            ((point.0 - x as f64) * server_config.width as f64) as i32,
            ((point.1 - y as f64) * server_config.width as f64) as i32,
        )
    };
    let b_px = tile2pixel(b_px);

    let b_bbox = overlay_coordinates.bbox.context("no bbox")?;
    let bbox0 = geo_trig::tile_index_float(z, b_bbox.bbox_0, b_bbox.bbox_1);
    let bbox1 = geo_trig::tile_index_float(z, b_bbox.bbox_2, b_bbox.bbox_3);
    let bbox0 = tile2pixel(bbox0);
    let bbox1 = tile2pixel(bbox1);
    let b_bbox = [bbox0, bbox1, (bbox1.0, bbox0.1), (bbox0.0, bbox1.1)];

    eprintln!("point: {:?}  bbox: {:?}", b_px, b_bbox);

    let img_bytes = spawn_blocking(move || {
        let mut img = img.into_rgb8();
        // let b_px: (i32, i32) = (127, 127);
        // let b_bbox: (i32, i32, i32, i32) = (32, 32, 172, 172);
        let line_len: i32 = 10;
        for pixel in img.enumerate_pixels_mut() {
            let current_pixel = (pixel.0 as i32, pixel.1 as i32);

            let hit_point_cross = |cxx: (i32, i32)| {
                (current_pixel.0 - cxx.0 == current_pixel.1 - cxx.1
                    && (current_pixel.0 - cxx.0).abs() <= line_len)
                    || (current_pixel.0 - cxx.0 == -current_pixel.1 + cxx.1
                        && (current_pixel.0 - cxx.0).abs() <= line_len)
            };

            if hit_point_cross(b_px) {
                *pixel.2 = pixel_max_contrast(pixel.2);
            }
            if current_pixel.0 == b_bbox[0].0
                || current_pixel.0 == b_bbox[1].0
                || current_pixel.1 == b_bbox[0].1
                || current_pixel.1 == b_bbox[1].1
            {
                *pixel.2 = pixel_max_contrast(pixel.2);
            }
        }

        let mut img_bytes: Vec<u8> = Vec::new();
        img.write_to(&mut Cursor::new(&mut img_bytes), image_format)
            .unwrap();
        img_bytes
    })
    .await?;
    let img_response = ImageResponse {
        img_bytes,
        content_type,
    };
    Ok(img_response)
}

fn pixel_max_contrast(px: &image::Rgb<u8>) -> image::Rgb<u8> {
    image::Rgb::<u8>([
        if px.0[0] > 127 { 0 } else { 255 },
        if px.0[1] > 127 { 0 } else { 255 },
        if px.0[2] > 127 { 0 } else { 255 },
    ])
}

#[rocket::main]
async fn main() -> rocket_anyhow::Result<()> {
    init_database().await?;

    // check we can run the manager once
    let _fetch_manager = tokio::spawn(fetch::fetch_loop());
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
            ],
        )
        .attach(Template::fairing())
        .launch()
        .await?;

    eprintln!("aborting worker loops...");
    _proxy_manager.abort();
    _fetch_manager.abort();
    eprintln!("clean exit done.");

    Ok(())
}
