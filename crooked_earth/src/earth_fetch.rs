use crate::bevy_tokio_tasks::TokioTasksRuntime;
use crate::config_tileserver::{self, TileServers};
use crate::geo_trig;
use crate::geo_trig::TileCoord;
use crate::util::get_current_timestamp;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::utils::hashbrown::HashSet;
use big_space::reference_frame::RootReferenceFrame;
use big_space::FloatingOrigin;
use rand::{thread_rng, Rng};
use reqwest::StatusCode;
use std::sync::Arc;

pub struct EarthFetchPlugin {}

impl Plugin for EarthFetchPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<DownloadPending>()
            .register_type::<DownloadStarted>()
            .register_type::<DownloadFinished>()
            .register_type::<TileFetchReceiver>()
            .register_type::<TileFetchSender>()
            .register_type::<WebMercatorTile>()
            .register_type::<WebMercatorTiledPlanet>()
            .register_type::<WebMercatorLeaf>()
            .register_type::<DebugMaterials>()
            .register_type::<TileSplitPls>()
            .register_type::<TileMergePls>()
            .register_type::<SpawnTilePls>()
            .add_systems(Update, spawn_root_planet_tiles)
            .add_systems(Update, insert_downloaded_planet_tiles)
            .add_systems(Update, start_planet_tile_download)
            .add_systems(Update, set_tiles_pending_when_planet_changes)
            .add_systems(
                Startup,
                (create_standard_material, spawn_tile_fetch_channel),
            )
            .add_systems(
                PostUpdate,
                (check_merge_or_split).after(
                    bevy::transform::TransformSystem::TransformPropagate,
                ),
            )
            .add_systems(PreUpdate, (spawn_tile_pls, split_tiles_pls, merge_tiles_pls));
    }
}

#[derive(Debug, Component, Default, Reflect)]
pub struct DownloadPending;

#[derive(Debug, Component, Reflect)]
// #[reflect(no_field_bounds)]
#[reflect(from_reflect = false)]
pub struct DownloadStarted(#[reflect(ignore)] tokio::task::JoinHandle<()>);

#[derive(Debug, Component, Default, Reflect)]
pub struct DownloadFinished;

#[derive(Resource, Deref, Reflect)]
#[reflect(from_reflect = false)]
struct TileFetchReceiver(
    #[reflect(ignore)] crossbeam_channel::Receiver<Arc<TileFetchResultData>>,
);

#[derive(Resource, Deref, Reflect)]
#[reflect(from_reflect = false)]
struct TileFetchSender(
    #[reflect(ignore)] crossbeam_channel::Sender<Arc<TileFetchResultData>>,
);

fn spawn_tile_fetch_channel(mut commands: Commands) {
    let (tx, rx) = crossbeam_channel::bounded(1000);
    commands.insert_resource(TileFetchReceiver(rx));
    commands.insert_resource(TileFetchSender(tx));
}

#[derive(Component, Debug, Clone, Reflect)]
pub struct WebMercatorTiledPlanet {
    pub planet_name: String,
    pub root_zoom_level: u8,
    pub tile_type: String,
    pub planet_radius: f64,
}

#[derive(Component, Debug, Clone, Reflect)]
pub struct WebMercatorTile {
    pub coord: geo_trig::TileCoord,
    pub parent_tile: Option<Entity>,
    pub parent_planet: Entity,
    pub children_tiles: Vec<Entity>,
    pub cartesian_diagonal: f64,
}

#[derive(Component, Debug, Clone, Reflect)]
pub struct WebMercatorLeaf {
    last_check: f64,
}

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

pub fn parse_bytes_to_image(
    img: bytes::Bytes,
    img_type: image::ImageFormat,
) -> Image {
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
    server: config_tileserver::TileServerConfig,
) -> TileFetchResultData {
    let img = parse_bytes_to_image(
        fetch_url_to_bytes(&server.get_tile_url(tile)).await,
        server.img_type(),
    );

    TileFetchResultData {
        image: img,
        target,
        tile,
        planet_info,
    }
}

#[derive(Debug, Clone, Resource, Reflect)]
struct DebugMaterials {
    _img1: Handle<Image>,
    mat1: Handle<StandardMaterial>,
}

fn create_standard_material(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let img1 = images.add(crate::util::uv_debug_texture());
    let mat1 = materials.add(StandardMaterial {
        base_color_texture: Some(img1.clone()),
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        alpha_mode: AlphaMode::Mask(0.5),
        ..default()
    });
    commands.insert_resource(DebugMaterials { _img1: img1, mat1 });
}

#[derive(Debug, Clone, Component, Reflect)]
struct SpawnTilePls {
    webtile: WebMercatorTile,
}

fn rand_float() -> f64 {
    thread_rng().gen_range(0.0..1.0)
}

fn spawn_tile_pls(
    q: Query<(Entity, &SpawnTilePls)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
    dbg_mat: Res<DebugMaterials>,
    space: Res<RootReferenceFrame<i64>>,
    tileservers: Res<TileServers>,
    planetinfo_q: Query<&WebMercatorTiledPlanet>,
) {
    // MACRO PLZ
    let mut total_tiles = 0;
    let t0 = crate::util::get_current_timestamp();

    for (target_ent, req) in q.into_iter().take(128) {
        total_tiles += 1;
        let planet_info = planetinfo_q.get(req.webtile.parent_planet).expect("planet not found");
        let tile = req.webtile.coord;
        let triangle_group =
            tile.geo_bbox().to_tris(planet_info.planet_radius);
        let mesh = triangle_group.generate_mesh();
        let tile_diagonal = triangle_group.diagonal();
        let mesh_handle = meshes.add(mesh);
        let tile_center = triangle_group.center();
        let downwards_level =
            tileservers.get(&planet_info.tile_type).max_level as f64
                - tile.z as f64;
        let tile_center =
            tile_center - tile_center.normalize() * downwards_level;

        let (tile_cell, tile_trans) = space.translation_to_grid(tile_center);

        let bundle = (
            Name::new(format!("{} {:?}", planet_info.planet_name, tile)),
            PbrBundle {
                mesh: mesh_handle,
                material: dbg_mat.mat1.clone(),
                transform: Transform::from_translation(tile_trans),
                visibility: Visibility::Visible,
                ..default()
            },
            tile_cell,
            WebMercatorTile {
                coord: tile,
                parent_planet: req.webtile.parent_planet,
                parent_tile: req.webtile.parent_tile,
                children_tiles: req.webtile.children_tiles.clone(),
                cartesian_diagonal: tile_diagonal as f64, // <<--- comes out bad from req
            },
            WebMercatorLeaf { last_check: get_current_timestamp() + rand_float() },
            DownloadPending,
        );
        commands
            .entity(target_ent)
            .remove::<SpawnTilePls>()
            .insert(bundle)
            .set_parent(req.webtile.parent_planet);
    }

    // MACRO PLZ
    if total_tiles > 0 {
        let dt_ms = (crate::util::get_current_timestamp() - t0) * 1000.0;
        let dt_ms = ((dt_ms * 1000.0) as i64) as f64 / 1000.0;
        if dt_ms > 1.0 {
            info!("spawned {} tiles in {} ms", total_tiles, dt_ms);
        }
    }
}

fn spawn_root_planet_tiles(
    planets_q: Query<
        (Entity, &WebMercatorTiledPlanet),
        Added<WebMercatorTiledPlanet>,
    >,
    mut commands: Commands,
) {
    if planets_q.is_empty() {
        return;
    }

    for (planet_ent, planet_info) in planets_q.iter() {
        for tile in
            geo_trig::TileCoord::get_root_tiles(planet_info.root_zoom_level)
        {
            commands.spawn(SpawnTilePls {
                webtile: WebMercatorTile {
                    coord: tile,
                    parent_planet: planet_ent,
                    parent_tile: None,
                    children_tiles: [].into(),
                    cartesian_diagonal: 0.0,
                },
            });
        }
    }
}

fn set_tiles_pending_when_planet_changes(
    planets_q: Query<
        (Entity, &WebMercatorTiledPlanet, &Children),
        Changed<WebMercatorTiledPlanet>,
    >,
    finished_q: Query<
        (),
        (
            With<DownloadFinished>,
            Without<DownloadStarted>,
            With<WebMercatorTile>,
        ),
    >,
    started_q: Query<&DownloadStarted, With<WebMercatorTile>>,
    mut commands: Commands,
) {
    for (_planet_ent, _planet_info, children) in planets_q.iter() {
        let mut current_count = 0;
        let mut aborted_count = 0;
        for child in children.iter() {
            if let Ok(()) = finished_q.get(*child) {
                commands
                    .entity(*child)
                    .remove::<(DownloadStarted, DownloadFinished)>()
                    .insert(DownloadPending);
                current_count += 1;
            } else if let Ok(started) = started_q.get(*child) {
                started.0.abort();
                commands
                    .entity(*child)
                    .remove::<(DownloadStarted, DownloadFinished)>()
                    .insert(DownloadPending);
                current_count += 1;
                aborted_count += 1;
            }
        }
        info!(
            "reset download for {} (aborted {}) tiles for planet {}",
            current_count, aborted_count, _planet_info.planet_name
        );
    }
}

fn start_planet_tile_download(
    pending_tiles: Query<
        (Entity, &WebMercatorTile, &Parent),
        With<DownloadPending>,
    >,
    planet_q: Query<&WebMercatorTiledPlanet>,
    tileservers: Res<TileServers>,
    sender: Res<TileFetchSender>,
    runtime: ResMut<TokioTasksRuntime>,
    mut commands: Commands,
) {
    let mut current_iter = 0;
    let t0 = crate::util::get_current_timestamp();
    if pending_tiles.is_empty() {
        return;
    }

    let dispatch_count: usize = 16;
    let (task_tx, task_rx) = crossbeam_channel::bounded(dispatch_count);

    for (target, tile, parent) in pending_tiles.iter().take(dispatch_count) {
        let planet_info = planet_q.get(parent.get()).expect("parent is planet");
        current_iter += 1;

        let planet_info = planet_info.clone();
        let sender = sender.clone();
        let server_config = tileservers.get(&planet_info.tile_type);
        let tile = tile.coord.clone();
        let task_tx = task_tx.clone();

        runtime.spawn_background_task(move |mut _ctx| async move {
            let tokio_handle = tokio::task::spawn(async move {
                let data =
                    fetch_tile_data(tile, target, planet_info, server_config)
                        .await;

                let _ = sender.send(Arc::new(data));
            });
            let _ = task_tx.send((target, tokio_handle));
        });
        if get_current_timestamp() - t0 > 0.001 {
            break;
        }
    }
    for (target, task_h) in task_rx.into_iter().take(current_iter) {
        commands
            .entity(target)
            .remove::<DownloadPending>()
            .remove::<DownloadFinished>()
            .insert(DownloadStarted(task_h));
    }

    if current_iter > 0 {
        let dt_ms = (crate::util::get_current_timestamp() - t0) * 1000.0;
        let dt_ms = ((dt_ms * 1000.0) as i64) as f64 / 1000.0;
        if dt_ms > 1.0 {
            info!("started download {} tiles in {} ms", current_iter, dt_ms);
        }
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
            .remove::<DownloadStarted>()
            .insert(DownloadFinished);
    }
    if current_iter > 0 {
        let dt_ms = (crate::util::get_current_timestamp() - t0) * 1000.0;
        let dt_ms = ((dt_ms * 1000.0) as i64) as f64 / 1000.0;
        if dt_ms > 1.0 {
            info!("inserted {} materials in {} ms", current_iter, dt_ms);
        }
    }
}

fn check_merge_or_split(
    leaf_q: Query<
        (Entity, &GlobalTransform, &WebMercatorTile, &WebMercatorLeaf),
        With<WebMercatorLeaf>,
    >,
    camera_q: Query<
        &GlobalTransform,
        (With<FloatingOrigin>, Without<WebMercatorLeaf>),
    >,
    planetinfo_q: Query<&WebMercatorTiledPlanet>,
    tileservers: Res<TileServers>,
    get_tileinfo_q: Query<&WebMercatorTile>,
    mut commands: Commands,
) {
    let camera_pos = camera_q.single().translation();
    let now = get_current_timestamp();
    const CHECK_INTERVAL_S: f64 = 1.0;
    let mut iter_count = 0;
    const SCREEN_COVERAGE_FOR_SPLIT: f32 = 0.3;
    for (leaf_ent, leaf_transform, tile_info, leaf_marker) in leaf_q.iter() {
        if now - leaf_marker.last_check < CHECK_INTERVAL_S {
            continue;
        }
        iter_count += 1;

        commands
            .entity(leaf_ent)
            .insert(WebMercatorLeaf { last_check: now + rand_float()});
        let leaf_pos = leaf_transform.translation();
        let dist_leaf_to_cam = (leaf_pos - camera_pos).length();
        let screen_coverage =
            tile_info.cartesian_diagonal as f32 / dist_leaf_to_cam;
        let planet_info = planetinfo_q
            .get(tile_info.parent_planet)
            .expect("parent of leaf is not planet");
        let tileserver = tileservers.get(&planet_info.tile_type);

        if screen_coverage > SCREEN_COVERAGE_FOR_SPLIT
            && tile_info.coord.z < tileserver.max_level
        {
            commands
                .entity(leaf_ent)
                .remove::<WebMercatorLeaf>()
                .insert(TileSplitPls);
        } else if tile_info.parent_tile.is_some()
            && (tile_info.coord.z > tileserver.max_level
                || (screen_coverage < SCREEN_COVERAGE_FOR_SPLIT / 4.0))
        {
            let parent = tile_info.parent_tile.unwrap();
            if let Ok(parent_info) = get_tileinfo_q.get(parent) {
                for child in parent_info.children_tiles.iter() {
                    commands.entity(*child).remove::<WebMercatorLeaf>();
                }
                commands.entity(parent).insert(TileMergePls);
            } else {
                warn!("want to merge {:?} {:?} but parent {:?} does not have tile!", leaf_ent,  tile_info.coord, parent);
                panic!("bagpl");
            }
        }
        if iter_count >= 128 {
            break;
        }
    }
}

#[derive(Debug, Component, Reflect)]
struct TileSplitPls;
#[derive(Debug, Component, Reflect)]
struct TileMergePls;

fn split_tiles_pls(
    leaf_q: Query<(Entity, &WebMercatorTile), With<TileSplitPls>>,
    mut commands: Commands,
) {
    for (leaf_ent, tile_info) in leaf_q.iter().take(64) {
        if !tile_info.children_tiles.is_empty() {
            warn!(
                "got TileSplitPlz on thing that already has children: {:?}.",
                leaf_ent
            );
            commands.entity(leaf_ent).remove::<TileSplitPls>();
            continue;
        }
        let mut new_leaf_tile = tile_info.clone();

        for child_tile in tile_info.coord.children() {
            let child_id = commands
                .spawn(SpawnTilePls {
                    webtile: WebMercatorTile {
                        coord: child_tile,
                        parent_planet: tile_info.parent_planet,
                        parent_tile: Some(leaf_ent),
                        children_tiles: [].into(),
                        cartesian_diagonal: 0.0,
                    },
                })
                .id();
            new_leaf_tile.children_tiles.push(child_id);
        }
        commands
            .entity(leaf_ent)
            .remove::<(WebMercatorLeaf, TileSplitPls)>()
            .insert(new_leaf_tile);
    }
}

fn merge_tiles_pls(
    q: Query<(Entity, &WebMercatorTile), With<TileMergePls>>,
    tileinfo_q: Query<&WebMercatorTile>,
    tilestarted_q: Query<&DownloadStarted>,
    mut commands: Commands,
) {
    let mut to_check = vec![];
    for (ent, tile_info) in q.iter().take(64) {
        if tile_info.children_tiles.is_empty() {
            warn!("empty children list for tile witih MergePls set: {:?}", ent);
            commands.entity(ent).remove::<TileMergePls>();
            continue;
        }
        for child_ent in tile_info.children_tiles.iter() {
            to_check.push(*child_ent);
        }
        let mut new_info = tile_info.clone();
        new_info.children_tiles.clear();
        commands
            .entity(ent)
            .remove::<TileMergePls>()
            .insert(WebMercatorLeaf {
                last_check: get_current_timestamp()+ rand_float(),
            })
            .insert(new_info);
    }

    let mut to_despawn = HashSet::new();
    while !to_check.is_empty() {
        let current = to_check.pop().unwrap();
        if let Ok(info) = tileinfo_q.get(current) {
            to_despawn.insert(current);
            for next in info.children_tiles.iter() {
                to_check.push(*next);
            }
        }
    }
    for t in to_despawn {
        if let Ok(started) = tilestarted_q.get(t) {
            started.0.abort();
        }
        commands.entity(t).despawn_recursive();
    }
}
