use crate::bevy_tokio_tasks::TokioTasksRuntime;
use crate::config_tileserver::{self, TileServers};
use crate::geo_trig;
use crate::geo_trig::TileCoord;
use crate::spawn_universe::TheCamera;
use crate::util::get_current_timestamp;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::utils::hashbrown::HashSet;
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
            .register_type::<CheckPostSplit>()
            .register_type::<CheckPostMerge>()
            .add_systems(Update, spawn_root_planet_tiles)
            .add_systems(Update, insert_downloaded_planet_tiles)
            .add_systems(Update, start_planet_tile_download)
            .add_systems(PostUpdate, set_tiles_pending_when_planet_changes)
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
            .add_systems(
                PostUpdate,
                (spawn_tile_pls, split_tiles_pls, merge_tiles_pls),
            )
            .add_systems(PreUpdate, (check_post_split, check_post_merge));
    }
}

#[derive(Debug, Component, Default, Reflect, Clone, Copy)]
pub struct DownloadPending {
    fail_cnt: i32,
    try_after: f64,
}

#[derive(Debug, Component, Reflect)]
// #[reflect(no_field_bounds)]
#[reflect(from_reflect = false)]
pub struct DownloadStarted {
    #[reflect(ignore)]
    abort_handle: tokio::task::JoinHandle<()>,
    pending_info: DownloadPending,
}

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

#[derive(Component, Debug, Clone, Reflect, Default)]
pub struct WebMercatorLeaf {
    check_after: f64,
}

#[derive(Debug)]
pub struct TileFetchResultData {
    image: Option<Image>,
    target: Entity,
    tile: TileCoord,
    planet_info: WebMercatorTiledPlanet,
    pending_info: DownloadPending,
}

async fn fetch_url_to_bytes(url: &str) -> Option<bytes::Bytes> {
    let resp = reqwest::get(url).await;
    if let Ok(resp) = resp {
        if resp.status() == StatusCode::OK {
            if let Ok(bytes) = resp.bytes().await {
                return Some(bytes);
            }
        }
    }
    None
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
    pending_info: DownloadPending,
) -> TileFetchResultData {
    let img = if let Some(img_bytes) =
        fetch_url_to_bytes(&server.get_tile_url(tile)).await
    {
        Some(parse_bytes_to_image(img_bytes, server.img_type()))
    } else {
        None
    };

    TileFetchResultData {
        image: img,
        target,
        tile,
        planet_info,
        pending_info,
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
    is_root: bool,
}

fn rand_float() -> f64 {
    thread_rng().gen_range(0.0..1.0)
}

fn spawn_tile_pls(
    q: Query<(Entity, &SpawnTilePls)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
    dbg_mat: Res<DebugMaterials>,
    tileservers: Res<TileServers>,
    planetinfo_q: Query<&WebMercatorTiledPlanet>,
    space_q: Query<&big_space::reference_frame::ReferenceFrame<i64>>,
    tileinfo_q: Query<&WebMercatorTile>,
) {
    // MACRO PLZ
    let mut total_tiles = 0;
    let t0 = crate::util::get_current_timestamp();

    for (target_ent, req) in q.into_iter().take(128) {
        if let Some(p) = req.webtile.parent_tile {
            if tileinfo_q.get(p).is_err() {
                warn!(
                    "cannot spawn {:?}; parent tile {:?} dissapeared",
                    req.webtile.coord, p
                );
                commands.entity(target_ent).despawn_recursive();
                continue;
            }
        }
        let planet_info = if let Ok(planet_info) =
            planetinfo_q.get(req.webtile.parent_planet)
        {
            planet_info
        } else {
            warn!(
                "cannot spawn {:?}; planet {:?} dissapeared",
                req.webtile.coord, req.webtile.parent_planet
            );
            commands.entity(target_ent).despawn_recursive();
            continue;
        };
        total_tiles += 1;
        let tile = req.webtile.coord;
        let triangle_group = tile.geo_bbox().to_tris(planet_info.planet_radius);
        let mesh = triangle_group.generate_mesh();
        let tile_diagonal = triangle_group.diagonal();
        let mesh_handle = meshes.add(mesh);
        let tile_center = triangle_group.center();
        let downwards_level = tileservers.get(&planet_info.tile_type).max_level
            as f64
            - tile.z as f64;
        let tile_center =
            tile_center - tile_center.normalize() * downwards_level;

        let space = space_q.get(req.webtile.parent_planet).unwrap();
        let (tile_cell, tile_trans) = space.translation_to_grid(tile_center);

        let bundle = (
            Name::new(format!("{} {:?}", planet_info.planet_name, tile)),
            PbrBundle {
                mesh: mesh_handle,
                material: dbg_mat.mat1.clone(),
                transform: Transform::from_translation(tile_trans),
                visibility: if req.is_root {
                    Visibility::Visible
                } else {
                    Visibility::Visible //Hidden !!
                },
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
            DownloadPending::default(),
            WebMercatorLeaf::default(),
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
            commands.spawn((
                Name::new("root tile pls"),
                SpawnTilePls {
                    webtile: WebMercatorTile {
                        coord: tile,
                        parent_planet: planet_ent,
                        parent_tile: None,
                        children_tiles: [].into(),
                        cartesian_diagonal: 0.0,
                    },
                    is_root: true,
                },
            ));
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
                    .insert(DownloadPending::default());
                current_count += 1;
            } else if let Ok(started) = started_q.get(*child) {
                started.abort_handle.abort();
                commands
                    .entity(*child)
                    .remove::<(DownloadStarted, DownloadFinished)>()
                    .insert(DownloadPending::default());
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
        (Entity, &WebMercatorTile, &Parent, &DownloadPending),
        With<DownloadPending>,
    >,
    running_tiles: Query<
        Entity,
        (With<WebMercatorTile>, With<DownloadStarted>),
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
    let running_count = running_tiles.iter().count() as i32;
    let max_iter = 221 - running_count;
    if max_iter <= 0 {
        return;
    }

    let dispatch_count: usize = 16;
    let (task_tx, task_rx) = crossbeam_channel::bounded(dispatch_count);

    // sort tiles after try_after time desc
    use rand::prelude::*;
    let mut rng = rand::thread_rng();
    let pending_tiles: Vec<_> = pending_tiles.iter().filter(|k| k.3.try_after < t0).collect::<Vec<_>>().choose_multiple(&mut rng, dispatch_count).cloned().collect();

    for (target, tile, parent, pending_info) in
        pending_tiles.into_iter()
    {
        if t0 < pending_info.try_after {
            continue;
        }
        let planet_info = planet_q.get(parent.get()).expect("parent is planet");
        current_iter += 1;

        let planet_info = planet_info.clone();
        let sender = sender.clone();
        let server_config = tileservers.get(&planet_info.tile_type);
        let tile = tile.coord.clone();
        let task_tx = task_tx.clone();
        let pending_info2 = pending_info.clone();

        runtime.spawn_background_task(move |mut _ctx| async move {
            let tokio_handle = tokio::task::spawn(async move {
                let data = fetch_tile_data(
                    tile,
                    target,
                    planet_info,
                    server_config,
                    pending_info2,
                )
                .await;

                let _ = sender.send(Arc::new(data));
            });
            let _ = task_tx.send((target, tokio_handle));
        });
        let (target, task_h) = task_rx.recv().expect("queue kaput");
        commands
            .entity(target)
            .remove::<DownloadPending>()
            .remove::<DownloadFinished>()
            .insert(DownloadStarted {
                abort_handle: task_h,
                pending_info: *pending_info,
            });
        if get_current_timestamp() - t0 > 0.001 {
            break;
        }
        if current_iter > max_iter {
            break;
        }
    }

    if current_iter > 0 {
        let dt_ms = (crate::util::get_current_timestamp() - t0) * 1000.0;
        let dt_ms = ((dt_ms * 1000.0) as i64) as f64 / 1000.0;
        if dt_ms > 1.5 {
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

        if message.image.is_none() {
            // curl failed => set new settings for pending
            let fail_cnt = message.pending_info.fail_cnt;
            commands
                .entity(message.target)
                .remove::<DownloadStarted>()
                .insert(DownloadPending {
                    fail_cnt: fail_cnt + 1,
                    try_after: get_current_timestamp()
                        + 0.1
                        + rand_float()
                        + 2.0f64.powi(fail_cnt),
                });
            continue;
        }

        current_iter += 1;
        let img_handle = images.add(message.image.unwrap());
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
    transform_q: Query<&GlobalTransform>,
    leaf_q: Query<(Entity, &WebMercatorLeaf)>,
    camera_q: Query<(Entity, &TheCamera)>,
    planetinfo_q: Query<&WebMercatorTiledPlanet>,
    tileinfo_q: Query<&WebMercatorTile>,
    tileservers: Res<TileServers>,
    get_tileinfo_q: Query<&WebMercatorTile>,
    mut commands: Commands,
) {
    use std::collections::HashMap;
    let mut _transform_hash = HashMap::<Entity, Vec3>::new();
    let mut get_global_transform = |ent| {
        if !_transform_hash.contains_key(&ent) {
            _transform_hash
                .insert(ent, transform_q.get(ent).unwrap().translation());
        }
        _transform_hash.get(&ent).unwrap().clone()
    };

    let camera_pos = get_global_transform(camera_q.single().0);
    let mut decide_split_or_merge = |tile_ent| {
        let leaf_pos = get_global_transform(tile_ent);
        let dist_leaf_to_cam = (leaf_pos - camera_pos).length();
        let tile_info = tileinfo_q.get(tile_ent).unwrap();
        let screen_coverage =
            tile_info.cartesian_diagonal as f32 / dist_leaf_to_cam;
        let planet_info = planetinfo_q.get(tile_info.parent_planet).unwrap();
        let planet_pos = get_global_transform(tile_info.parent_planet);

        let screen_ang_cos = (camera_pos - planet_pos)
            .normalize()
            .dot((leaf_pos - planet_pos).normalize());
        let screen_coverage = screen_coverage * screen_ang_cos;
        let tileserver = tileservers.get(&planet_info.tile_type);
        let should_split = screen_coverage > SCREEN_COVERAGE_FOR_SPLIT
            && tile_info.coord.z < tileserver.max_level;
        let should_merge = tile_info.parent_tile.is_some()
            && (tile_info.coord.z > tileserver.max_level
                || (screen_coverage < SCREEN_COVERAGE_FOR_SPLIT / 4.0));

        (should_split, should_merge, tile_info.parent_tile)
    };

    let now = get_current_timestamp();
    const CHECK_INTERVAL_S: f64 = 1.0;
    let mut iter_count = 0;
    const SCREEN_COVERAGE_FOR_SPLIT: f32 = 0.3;
    
    use rand::prelude::*;
    let mut rng = rand::thread_rng();
    let leaf_q2: Vec<_> = leaf_q.iter().filter(|k| k.1.check_after < now).collect::<Vec<_>>().choose_multiple(&mut rng, 128).cloned().collect();

    let mut merge_set = HashSet::<Entity>::new();
    for (leaf_ent, leaf_marker) in leaf_q2.into_iter() {
        if now < leaf_marker.check_after {
            continue;
        }
        if iter_count >= 128 {
            break;
        }

        if let Some(p) = tileinfo_q.get(leaf_ent).unwrap().parent_tile {
            if tileinfo_q.get(p).is_err() {
                warn!("leaf {:?}; parent tile {:?} dissapeared", leaf_ent, p);
                commands.entity(leaf_ent).despawn_recursive();
                continue;
            }
        }

        iter_count += 1;
        let (should_split, should_merge, maybe_parent) =
            decide_split_or_merge(leaf_ent);

        if should_split {
            commands
                .entity(leaf_ent)
                .remove::<WebMercatorLeaf>()
                .insert(TileSplitPls);
            // warn!("check/split pls: {:?}", leaf_ent);
        } else if should_merge {
            let parent = maybe_parent.unwrap();
            merge_set.insert(parent);
        } else {
            commands.entity(leaf_ent).insert(WebMercatorLeaf {
                check_after: now + 0.1 * rand_float() + CHECK_INTERVAL_S,
            });
        }
    }
    for parent in merge_set.into_iter() {
        if let Ok(parent_info) = get_tileinfo_q.get(parent) {
            let mut all_children_are_leafs = true;
            for child in parent_info.children_tiles.iter() {
                if leaf_q.get(*child).is_err() {
                    all_children_are_leafs = false;
                    break;
                }
            }
            if !all_children_are_leafs {
                continue;
            }
            let (_parent_split, _, _) = decide_split_or_merge(parent);
            if _parent_split {
                continue;
            }
            for child in parent_info.children_tiles.iter() {
                commands.entity(*child).remove::<WebMercatorLeaf>();
            }
            // warn!("check/merge pls: {:?}", parent);
            commands.entity(parent).insert(TileMergePls);
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
                .spawn((
                    Name::new("spawn more tiles plz"),
                    SpawnTilePls {
                        webtile: WebMercatorTile {
                            coord: child_tile,
                            parent_planet: tile_info.parent_planet,
                            parent_tile: Some(leaf_ent),
                            children_tiles: [].into(),
                            cartesian_diagonal: 0.0,
                        },
                        is_root: false,
                    },
                ))
                .id();
            new_leaf_tile.children_tiles.push(child_id);
        }
        commands
            .entity(leaf_ent)
            .remove::<TileSplitPls>()
            .remove::<WebMercatorLeaf>()
            .insert(new_leaf_tile)
            .insert(CheckPostSplit::default());
        // warn!("split tile done {:?}", leaf_ent);
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
            .insert(WebMercatorLeaf::default())
            .insert(new_info)
            .insert(Visibility::Visible);
        // warn!("merge tiles done {:?}", ent);
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
            started.abort_handle.abort();
        }
        commands.entity(t).despawn_recursive();
    }
}

#[derive(Debug, Component, Reflect, Default)]
struct CheckPostSplit {
    next_check_at: f64,
}

#[derive(Debug, Component, Reflect)]
struct CheckPostMerge;

fn check_post_split(
    mut new_parent_q: Query<(Entity, &WebMercatorTile, &mut CheckPostSplit)>,
    tileinfo_q: Query<&WebMercatorTile>,
    download_finished_q: Query<&DownloadFinished>,
    mut commands: Commands,
    // dbg_mat: Res<DebugMaterials>,
) {
    let mut i = 0;
    for (parent_ent, parent_tile, mut check) in new_parent_q.iter_mut() {
        if i > 128 {
            break;
        }
        if check.next_check_at > get_current_timestamp() {
            continue;
        }
        check.next_check_at =
            get_current_timestamp() + rand_float() * 0.1 + 0.1;
        i += 1;

        let mut all_downloaded = true;
        let mut child_dissapeared = false;
        for child in parent_tile.children_tiles.iter() {
            if tileinfo_q.get(*child).is_err() {
                child_dissapeared = true;
                break;
            }
            if download_finished_q.get(*child).is_err() {
                all_downloaded = false;
                break;
            }
        }
        if child_dissapeared {
            continue;
        }
        if parent_tile.children_tiles.is_empty() {
            // warn!("checking post split but no children: {:?} dissapear={} empty={}", parent_ent, child_dissapeared, parent_tile.children_tiles.is_empty() );
            commands.entity(parent_ent).remove::<CheckPostSplit>();
            continue;
        }
        if !all_downloaded {
            continue;
        }
        // info!("check success split {:?}", parent_ent);
        commands
            .entity(parent_ent)
            .remove::<CheckPostSplit>()
            .insert(Visibility::Visible); // !!! Hidden
        for child in parent_tile.children_tiles.iter() {
            commands.entity(*child).insert((Visibility::Visible,));
        }
    }
}

fn check_post_merge(
    q: Query<(Entity, &WebMercatorTile), With<CheckPostMerge>>,
    tileinfo_q: Query<&WebMercatorTile>,
    tilestarted_q: Query<&DownloadStarted>,
    mut commands: Commands,
) {
}
