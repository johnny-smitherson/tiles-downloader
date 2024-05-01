use crate::bevy_tokio_tasks::TokioTasksRuntime;
use crate::geo_trig;
use bevy::prelude::*;
use bevy::render::mesh::{self, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;

pub struct EarthFetchPlugin {}

impl Plugin for EarthFetchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_load_tasks);
    }
}

async fn get_tile(tile: geo_trig::TileCoord) -> (Mesh, Image) {
    let mesh = geo_trig::generate_mesh(tile.geo_bbox().to_tris());

    let tile_url = format!(
        "http://localhost:8000/api/tile/arcgis_sat/{}/{}/{}/tile.jpg",
        tile.z, tile.x, tile.y
    );
    let img = reqwest::get(tile_url).await.unwrap().bytes().await.unwrap();
    info!("downlaoded {:?}: {} bytes", &tile, img.len());

    let img_reader =
        image::io::Reader::with_format(std::io::Cursor::new(img), image::ImageFormat::Jpeg);
    let img = img_reader.decode().unwrap();

    use bevy::render::render_asset::RenderAssetUsages;
    let img = Image::from_dynamic(
        img,
        false,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    (mesh, img)
}

use bevy::{
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use rand::Rng;

/// Creates a colorful test pattern
fn uv_debug_texture() -> Image {
    const TEXTURE_SIZE: usize = 8;

    let mut palette: [u8; 32] = [
        255, 102, 159, 255, 255, 159, 102, 255, 236, 255, 102, 255, 121, 255, 102, 255, 102, 255,
        198, 255, 102, 198, 255, 255, 121, 102, 255, 255, 236, 102, 255, 255,
    ];

    let mut texture_data = [0; TEXTURE_SIZE * TEXTURE_SIZE * 4];
    for y in 0..TEXTURE_SIZE {
        let offset = TEXTURE_SIZE * y * 4;
        texture_data[offset..(offset + TEXTURE_SIZE * 4)].copy_from_slice(&palette);
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

fn setup_load_tasks(runtime: ResMut<TokioTasksRuntime>) {
    for tile in geo_trig::init_tiles() {
        let t2 = tile.clone();
        runtime.spawn_background_task(move |mut ctx| async move {
            let (mesh, image) = get_tile(t2).await;
            ctx.sleep_updates(1).await;

            let mesh_handle = ctx
                .run_on_main_thread(move |ctx| {
                    let mut meshes = ctx.world.get_resource_mut::<Assets<Mesh>>().unwrap();
                    let mesh_handle = meshes.add(mesh);
                    mesh_handle
                })
                .await;

            ctx.sleep_updates(1).await;

            let image_handle = ctx
                .run_on_main_thread(move |ctx| {
                    let mut images = ctx.world.get_resource_mut::<Assets<Image>>().unwrap();

                    let image_handle = images.add(image);
                    image_handle
                })
                .await;

            ctx.sleep_updates(1).await;

            let mat_handle = ctx
                .run_on_main_thread(move |ctx| {
                    let mut materials = ctx
                        .world
                        .get_resource_mut::<Assets<StandardMaterial>>()
                        .unwrap();

                    let mat_handle = materials.add(StandardMaterial {
                        base_color_texture: Some(image_handle),
                        ..default()
                    });
                    mat_handle
                })
                .await;

            ctx.sleep_updates(1).await;

            info!("assets loaded for {:?}", &tile);
            ctx.run_on_main_thread(move |ctx| {
                let bundle = PbrBundle {
                    mesh: mesh_handle,
                    material: mat_handle,
                    ..default()
                };
                ctx.world.spawn_empty().insert(bundle);

                info!("bundle inserted for {:?}", &tile);
            })
            .await;
        });
    }
}
