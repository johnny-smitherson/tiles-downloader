use crate::bevy_tokio_tasks::TokioTasksRuntime;
use crate::geo_trig;
use crate::geo_trig::TileCoord;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use rand::Rng;
use reqwest::StatusCode;
use std::sync::Arc;

pub struct EarthFetchPlugin {}

impl Plugin for EarthFetchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, begin_fetch_root_planet_tiles)
            .add_systems(Update, spawn_ready_planet_tiles)
            .add_systems(Startup, spawn_tile_fetch_channel);
    }
}

#[derive(Resource, Deref)]
struct TileFetchReceiver(crossbeam_channel::Receiver<Arc<TileFetchResultData>>);

#[derive(Resource, Deref)]
struct TileFetcSender(crossbeam_channel::Sender<Arc<TileFetchResultData>>);

fn spawn_tile_fetch_channel(mut commands: Commands) {
    let (tx, rx) = crossbeam_channel::bounded(1000);
    commands.insert_resource(TileFetchReceiver(rx));
    commands.insert_resource(TileFetcSender(tx));
}

#[derive(Component, Debug, Clone)]
pub struct WebMercatorTiledPlanet {
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
    mesh: Mesh,
    origin: Vec3,
    image: Image,
    parent: Entity,
    tile_coord: TileCoord,
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
    tile_type: &str,
    planet_radius: f64,
    parent: Entity,
    tile_coord: TileCoord,
) -> TileFetchResultData {
    let tile_url = format!(
        "http://localhost:8000/api/tile/{}/{}/{}/{}/tile.jpg",
        tile_type, tile.z, tile.x, tile.y
    );
    let img = fetch_url_image(&tile_url, image::ImageFormat::Jpeg).await;

    let triangle_group = tile.geo_bbox().to_tris(planet_radius);
    let mesh = triangle_group.generate_mesh();
    let tile_center = triangle_group.center();

    TileFetchResultData {
        mesh,
        image: img,
        origin: tile_center,
        parent,
        tile_coord,
    }
}

fn begin_fetch_root_planet_tiles(
    runtime: ResMut<TokioTasksRuntime>,
    planets_q: Query<
        (Entity, &WebMercatorTiledPlanet),
        Added<WebMercatorTiledPlanet>,
    >,
    sender: Res<TileFetcSender>,
) {
    for (planet_ent, planet_info) in planets_q.iter() {
        for tile in
            geo_trig::TileCoord::get_root_tiles(planet_info.root_zoom_level)
        {
            let t2 = tile;
            let planet_info = planet_info.clone();
            let sender = sender.clone();
            runtime.spawn_background_task(move |mut _ctx| async move {
                let tile_data = fetch_tile_data(
                    t2,
                    &planet_info.tile_type,
                    planet_info.planet_radius,
                    planet_ent,
                    tile,
                )
                .await;
                let _ = sender.send(Arc::new(tile_data));
            });
        }
    }
}

fn spawn_ready_planet_tiles(
    receiver: Res<TileFetchReceiver>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let max_iters = 64_i32;
    let mut current_iter = 0;
    let t0 = crate::util::get_current_timestamp();
    for _i in 1..=max_iters {
        if let Ok(message) = receiver.try_recv() {
            current_iter += 1;
            let message = Arc::try_unwrap(message).unwrap();
            // info!("running tile spawn {:?}", message);
            let mesh_handle = meshes.add(message.mesh);
            let img_handle = images.add(message.image);
            let mat_handle = materials.add(StandardMaterial {
                base_color_texture: Some(img_handle),
                perceptual_roughness: 1.0,
                reflectance: 0.0,
                ..default()
            });

            let bundle = (
                Name::new(format!("Planet Tile {:?}", message.tile_coord.clone())),
                PbrBundle {
                    mesh: mesh_handle,
                    material: mat_handle,
                    transform: Transform::from_translation(message.origin),
                    ..default()
                },
                big_space::GridCell::<i64>::ZERO,
                WebMercatorTile {
                    coord: message.tile_coord.clone(),
                },
                WebMercatorLeaf,
            );
            commands.spawn(bundle).set_parent(message.parent);
        } else {
            break;
        }
    }
    if current_iter > 0 {
        let dt_ms = (crate::util::get_current_timestamp() - t0) * 1000.0;
        let dt_ms = ((dt_ms * 1000.0) as i64) as f64 / 1000.0;
        info!("injected {} img in {} ms", current_iter, dt_ms);
    }
}
