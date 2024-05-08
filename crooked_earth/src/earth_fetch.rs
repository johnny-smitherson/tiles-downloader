use crate::bevy_tokio_tasks::TokioTasksRuntime;
use crate::diagnostics::{DownloadFinished, DownloadPending};
use crate::geo_trig;
use crate::geo_trig::TileCoord;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use reqwest::StatusCode;
use std::sync::Arc;

pub struct EarthFetchPlugin {}

impl Plugin for EarthFetchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, begin_fetch_root_planet_tiles)
            .add_systems(Update, insert_downloaded_planet_tiles)
            .add_systems(Startup, spawn_tile_fetch_channel);
    }
}

#[derive(Resource, Deref)]
struct TileFetchReceiver(crossbeam_channel::Receiver<Arc<TileFetchResultData>>);

#[derive(Resource, Deref)]
struct TileFetchSender(crossbeam_channel::Sender<Arc<TileFetchResultData>>);

fn spawn_tile_fetch_channel(mut commands: Commands) {
    let (tx, rx) = crossbeam_channel::bounded(1000);
    commands.insert_resource(TileFetchReceiver(rx));
    commands.insert_resource(TileFetchSender(tx));
}

#[derive(Component, Debug, Clone)]
pub struct WebMercatorTiledPlanet {
    pub planet_name: String,
    pub root_zoom_level: u8,
    pub tile_type: String,
    pub planet_radius: f64,
}

#[derive(Component, Debug, Clone)]
pub struct WebMercatorTile {
    pub coord: geo_trig::TileCoord,
}

#[derive(Component, Debug, Clone)]
pub struct WebMercatorLeaf;

#[derive(Debug)]
pub struct TileFetchResultData {
    image: Image,
    target: Entity,
    tile: TileCoord,
    planet_info: WebMercatorTiledPlanet,
}

async fn fetch_url_to_bytes(url: &str) -> bytes::Bytes {
    let img = {
        let mut current_wait = 1.0;
        let mut print_count: i32 = 0;
        loop {
            let resp = reqwest::get(url).await;
            if let Ok(resp) = resp {
                if resp.status() == StatusCode::OK {
                    if let Ok(bytes) = resp.bytes().await {
                        break bytes;
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs_f64(
                current_wait,
            ))
            .await;
            current_wait *= 1.1;
            current_wait += 0.1;
            if current_wait > 20.0 {
                current_wait = 20.0;
                if print_count % 4 == 0 {
                    warn!(
                        "file still not downloaded while at max wait: {:?}",
                        &url
                    );
                }
                print_count += 1;
            }
        }
    };
    // info!("downlaoded {:?}: {} bytes", &tile, img.len());
    img
}

async fn fetch_url_image(url: &str, img_type: image::ImageFormat) -> Image {
    let img = fetch_url_to_bytes(url).await;

    let img_reader =
        image::io::Reader::with_format(std::io::Cursor::new(img), img_type);
    let img = img_reader.decode().unwrap();

    let img = Image::from_dynamic(
        img,
        false,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    img
}

async fn fetch_tile_data(
    tile: geo_trig::TileCoord,
    target: Entity,
    planet_info: WebMercatorTiledPlanet,
) -> TileFetchResultData {
    let tile_url = format!(
        "http://localhost:8000/api/tile/{}/{}/{}/{}/tile.jpg",
        planet_info.tile_type, tile.z, tile.x, tile.y
    );
    let img = fetch_url_image(&tile_url, image::ImageFormat::Jpeg).await;

    TileFetchResultData {
        image: img,
        target,
        tile,
        planet_info,
    }
}

fn begin_fetch_root_planet_tiles(
    runtime: ResMut<TokioTasksRuntime>,
    planets_q: Query<
        (Entity, &WebMercatorTiledPlanet),
        Added<WebMercatorTiledPlanet>,
    >,
    sender: Res<TileFetchSender>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    if planets_q.is_empty() {
        return;
    }
    let default_img_handle = images.add(crate::util::uv_debug_texture());
    let default_mat_handle = materials.add(StandardMaterial {
        base_color_texture: Some(default_img_handle),
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..default()
    });

    for (planet_ent, planet_info) in planets_q.iter() {
        let mut total_tiles = 0;
        let t0 = crate::util::get_current_timestamp();
        for tile in
            geo_trig::TileCoord::get_root_tiles(planet_info.root_zoom_level)
        {
            total_tiles += 1;
            let triangle_group =
                tile.geo_bbox().to_tris(planet_info.planet_radius);
            let mesh = triangle_group.generate_mesh();
            let tile_center = triangle_group.center();
            let mesh_handle = meshes.add(mesh);

            let bundle = (
                Name::new(format!("{} {:?}", planet_info.planet_name, tile)),
                PbrBundle {
                    mesh: mesh_handle,
                    material: default_mat_handle.clone(),
                    transform: Transform::from_translation(tile_center),
                    ..default()
                },
                big_space::GridCell::<i64>::ZERO,
                WebMercatorTile { coord: tile },
                WebMercatorLeaf,
                DownloadPending,
            );
            let target = commands.spawn(bundle).set_parent(planet_ent).id();

            let planet_info = planet_info.clone();
            let sender = sender.clone();
            runtime.spawn_background_task(move |mut _ctx| async move {
                let tile_data =
                    fetch_tile_data(tile, target, planet_info).await;
                let _ = sender.send(Arc::new(tile_data));
            });
        }
        let dt_ms = (crate::util::get_current_timestamp() - t0) * 1000.0;
        let dt_ms = ((dt_ms * 1000.0) as i64) as f64 / 1000.0;
        info!(
            "spawned {} tiles in {} ms for planet {:?}",
            total_tiles, dt_ms, planet_info
        );
    }
}

fn insert_downloaded_planet_tiles(
    receiver: Res<TileFetchReceiver>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    tile_q: Query<&WebMercatorTile>,
    planetinfo_q: Query<&WebMercatorTiledPlanet>,
    parent_q: Query<&Parent>,
) {
    let max_iters = 64;
    let mut current_iter = 0;
    let t0 = crate::util::get_current_timestamp();
    for _i in 1..=max_iters {
        let message = if let Ok(message) = receiver.try_recv() {
            Arc::try_unwrap(message).unwrap()
        } else {
            break;
        };
        let target_tile = tile_q.get(message.target);
        if target_tile.is_err() {
            warn!(
                "cannot find entity {:?} for downloaded tile {:?}",
                message.target, message.tile
            );
            continue;
        }
        assert!(
            target_tile.unwrap().coord.eq(&message.tile),
            "wrong tile for this entity"
        );
        let target_parent = parent_q.get(message.target);
        if target_parent.is_err() {
            warn!("cannot find parent for entity {:?}", message.target);
            continue;
        }
        let target_planet = planetinfo_q
            .get(target_parent.unwrap().get())
            .expect("parent is not planet");
        assert!(
            target_planet
                .planet_name
                .eq(&message.planet_info.planet_name),
            "wrong planet"
        );
        if !target_planet.tile_type.eq(&message.planet_info.tile_type) {
            warn!("fetched tile type {} is not the one currently set on planet {} ({}).",
            &message.planet_info.tile_type,target_planet.planet_name, target_planet.tile_type );
            continue;
        }

        current_iter += 1;
        let img_handle = images.add(message.image);
        let mat_handle = materials.add(StandardMaterial {
            base_color_texture: Some(img_handle),
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            ..default()
        });
        commands
            .entity(message.target)
            .insert(mat_handle)
            .remove::<DownloadPending>()
            .insert(DownloadFinished);
    }
    if current_iter > 0 {
        let dt_ms = (crate::util::get_current_timestamp() - t0) * 1000.0;
        let dt_ms = ((dt_ms * 1000.0) as i64) as f64 / 1000.0;
        info!("spawned {} tiles in {} ms", current_iter, dt_ms);
    }
}
