use crate::config;
use crate::download_geoduck;
use crate::download_geosearch;
use crate::download_tile;
use crate::download_tile::OverlayDrawCoordinates;
use crate::rocket_anyhow;
use anyhow::Context;
use rocket::fs::NamedFile;
use rocket::http::ContentType;
use rocket::response::Responder;
use rocket::Response;
use std::io::Cursor;

pub fn get_api_routes() -> Vec<rocket::Route> {
    routes![
        get_tile,
        get_tile_with_overlay,
        geo_search_json,
        get_overt_geoduck
    ]
}

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

#[get("/api/geo/<q_location>/json")]
async fn geo_search_json(q_location: &str) -> rocket_anyhow::Result<NamedFile> {
    let geojson_path =
        download_geosearch::search_geojson_to_disk(q_location).await?;

    Ok(NamedFile::open(&geojson_path).await.with_context(|| {
        format!("file missing from disk: {:?}", &geojson_path)
    })?)
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
    let path = download_tile::get_tile(server_name, x, y, z, extension).await?;

    Ok(Some(NamedFile::open(&path).await.with_context(|| {
        format!("file missing from disk: {:?}", &path)
    })?))
}

#[get("/api/overt_geoduck/<theme>/<o_type>/<z>/<x>/<y>/overt.parquet")]
async fn get_overt_geoduck(
    theme: &str,
    o_type: &str,
    x: u64,
    y: u64,
    z: u8,
) -> rocket_anyhow::Result<Option<NamedFile>> {
    let path =
        download_geoduck::download_geoduck_to_disk(theme, o_type, x, y, z)
            .await?;
    Ok(Some(NamedFile::open(&path).await.with_context(|| {
        format!("file missing from disk: {:?}", &path)
    })?))
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
    let path = download_tile::get_tile(server_name, x, y, z, extension).await?;
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
