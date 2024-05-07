use crate::bevy_tokio_tasks::TokioTasksRuntime;
use crate::geo_trig;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat,
};
use rand::Rng;
use reqwest::StatusCode;

pub struct EarthFetchPlugin {}

impl Plugin for EarthFetchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_root_planet_tiles);
    }
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

#[derive(Debug)]
pub struct TileFetchData {
    mesh: Mesh,
    origin: Vec3,
    image: Image,
}

async fn get_tile(
    tile: geo_trig::TileCoord,
    tile_type: &str,
    planet_radius: f64,
) -> TileFetchData {
    let triangle_group = tile.geo_bbox().to_tris(planet_radius);
    let mesh = triangle_group.generate_mesh();
    let tile_center = triangle_group.center();

    let tile_url = format!(
        "http://localhost:8000/api/tile/{}/{}/{}/{}/tile.jpg",
        tile_type, tile.z, tile.x, tile.y
    );
    let img = {
        let mut current_wait = 1.0;
        let mut print_count: i32 = 0;
        loop {
            let resp = reqwest::get(&tile_url).await;
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
                        "tile still not downloaded while at max wait: {:?}",
                        &tile
                    );
                }
                print_count += 1;
            }
        }
    };
    // info!("downlaoded {:?}: {} bytes", &tile, img.len());

    let img_reader = image::io::Reader::with_format(
        std::io::Cursor::new(img),
        image::ImageFormat::Jpeg,
    );
    let img = img_reader.decode().unwrap();

    let img = Image::from_dynamic(
        img,
        false,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    TileFetchData {
        mesh,
        image: img,
        origin: tile_center,
    }
}

/// Creates a colorful test pattern
#[allow(dead_code)]
fn uv_debug_texture() -> Image {
    const TEXTURE_SIZE: usize = 8;

    let mut palette: [u8; 32] = [
        255, 102, 159, 255, 255, 159, 102, 255, 236, 255, 102, 255, 121, 255,
        102, 255, 102, 255, 198, 255, 102, 198, 255, 255, 121, 102, 255, 255,
        236, 102, 255, 255,
    ];

    let mut texture_data = [0; TEXTURE_SIZE * TEXTURE_SIZE * 4];
    for y in 0..TEXTURE_SIZE {
        let offset = TEXTURE_SIZE * y * 4;
        texture_data[offset..(offset + TEXTURE_SIZE * 4)]
            .copy_from_slice(&palette);
        palette.rotate_right(4);
    }
    Image::new_fill(
        Extent3d {
            width: TEXTURE_SIZE as u32,
            height: TEXTURE_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &texture_data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

use crate::bevy_tokio_tasks::TaskContext;
async fn rand_sleep(ctx: &mut TaskContext) {
    let _rand_sleep = (&mut rand::thread_rng()).gen_range(1..3);
    ctx.sleep_updates(_rand_sleep).await;
}

fn spawn_root_planet_tiles(
    runtime: ResMut<TokioTasksRuntime>,
    planets_q: Query<
        (Entity, &WebMercatorTiledPlanet),
        Added<WebMercatorTiledPlanet>,
    >,
) {
    for (planet_ent, planet_info) in planets_q.iter() {
        for tile in
            geo_trig::TileCoord::get_root_tiles(planet_info.root_zoom_level)
        {
            let t2 = tile;
            let planet_info = planet_info.clone();
            runtime.spawn_background_task(move |mut ctx| async move {
                rand_sleep(&mut ctx).await;

                let tile_data = get_tile(
                    t2,
                    &planet_info.tile_type,
                    planet_info.planet_radius,
                )
                .await;

                rand_sleep(&mut ctx).await;

                let mesh_handle = ctx
                    .run_on_main_thread(move |ctx| {
                        let mut meshes = ctx
                            .world
                            .get_resource_mut::<Assets<Mesh>>()
                            .unwrap();

                        meshes.add(tile_data.mesh)
                    })
                    .await;

                rand_sleep(&mut ctx).await;

                let image_handle = ctx
                    .run_on_main_thread(move |ctx| {
                        let mut images = ctx
                            .world
                            .get_resource_mut::<Assets<Image>>()
                            .unwrap();

                        images.add(tile_data.image)
                    })
                    .await;

                rand_sleep(&mut ctx).await;

                let mat_handle = ctx
                    .run_on_main_thread(move |ctx| {
                        let mut materials = ctx
                            .world
                            .get_resource_mut::<Assets<StandardMaterial>>()
                            .unwrap();

                        materials.add(StandardMaterial {
                            base_color_texture: Some(image_handle),

                            perceptual_roughness: 1.0,
                            reflectance: 0.0,
                            ..default()
                        })
                    })
                    .await;

                rand_sleep(&mut ctx).await;

                // info!("assets loaded for {:?}", &tile);
                ctx.run_on_main_thread(move |ctx| {
                    let bundle = (
                        Name::new(format!("Planet Tile {:?}", tile.clone())),
                        PbrBundle {
                            mesh: mesh_handle,
                            material: mat_handle,
                            transform: Transform::from_translation(
                                tile_data.origin,
                            ),
                            ..default()
                        },
                        big_space::GridCell::<i64>::ZERO,
                        WebMercatorTile {
                            coord: tile.clone(),
                        },
                    );
                    ctx.world
                        .spawn_empty()
                        .insert(bundle)
                        .set_parent(planet_ent);

                    // info!("bundle inserted for {:?}", &tile);
                })
                .await;
            });
        }
    }
}
