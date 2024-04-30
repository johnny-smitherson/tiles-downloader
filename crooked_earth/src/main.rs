//! A simple 3D scene with light shining over a cube sitting on a plane.

use std::f64::consts::PI;

use bevy::prelude::*;
mod bevy_tokio_tasks;
use bevy_tokio_tasks::{TokioTasksPlugin, TokioTasksRuntime};

fn main() {
    App::new()
        // .add_plugins(bevy_web_asset::WebAssetPlugin)
        .add_plugins(DefaultPlugins)
        .add_plugins(TokioTasksPlugin::default())
        .add_plugins(bevy_trackball::TrackballPlugin)
        .add_systems(Startup,( setup, setup_load_tasks))
        .run();
}

#[derive(Reflect, Debug, Clone, Copy)]
struct TileCoord {
    x: u64,
    y: u64,
    z: u8,
}

#[derive(Reflect, Debug, Clone, Copy)]
struct GeoBBox {
    lon_west: f64,
    lat_south: f64,
    lon_east: f64,
    lat_north: f64,
}

impl GeoBBox {
    fn to_tris(self: &Self) -> Vec<TriangleData> {
        // 1 2
        // 3 4 ;  1-3-2  2-3-4
        let uv1 = Vec2::ZERO;
        let uv2 = Vec2::X;
        let uv3 = Vec2::Y;
        let uv4 = Vec2::X + Vec2::Y;

        let p1 = gps_to_cartesian(self.lon_west, self.lat_north);
        let p2 = gps_to_cartesian(self.lon_east, self.lat_north);
        let p3 = gps_to_cartesian(self.lon_west, self.lat_south);
        let p4 = gps_to_cartesian(self.lon_east, self.lat_south);
        vec![
            TriangleData::new([p1, p3, p2], [uv1, uv3, uv2]),
            TriangleData::new([p2, p3, p4], [uv2, uv3, uv4]),
        ]
        // vec![TriangleData::new([p1,p2,p3], [uv1, uv2, uv3]), TriangleData::new([p2,p4,p3], [uv2, uv4, uv3]),]
    }
}

fn gps_to_cartesian(lon_deg: f64, lat_deg: f64) -> Vec3 {
    // Vec3 {
    //     x:(lat) as f32/360.0,
    //     y:(lon) as f32/360.0,
    //     z:(0) as f32
    // }
    let lat = lat_deg.to_radians();
    let lon = lon_deg.to_radians();

    Vec3 {
        x: -(lat.cos() * lon.cos()) as f32,
        z: (lat.cos() * lon.sin()) as f32,
        y: (lat.sin()) as f32,
    }
}

#[derive(Reflect, Debug, Clone, Copy)]
pub struct TriangleData {
    verts: [Vec3; 3],
    uvs: [Vec2; 3],
    norm: [Vec3; 3],
    center: Vec3,
    max_edge_len: f32,
    min_edge_len: f32,
}
impl TriangleData {
    fn new(verts: [Vec3; 3], uvs: [Vec2; 3]) -> Self {
        let mut rng = rand::thread_rng();

        // let v12 = verts[2] - verts[1];
        // let v01 = verts[1] - verts[0];
        // let norm = -v12.cross(v01).normalize();
        // let normals = [norm, norm, norm];
        let normals = [
            verts[0].normalize(),
            verts[1].normalize(),
            verts[2].normalize(),
        ];

        let l1 = (verts[0] - verts[1]).length();
        let l2 = (verts[2] - verts[1]).length();
        let l3 = (verts[0] - verts[2]).length();

        Self {
            verts,
            uvs,
            norm: normals,
            center: (verts[0] + verts[1] + verts[2]) / 3.0,
            max_edge_len: max3(l1, l2, l3),
            min_edge_len: min3(l1, l2, l3),
        }
    }
}

pub fn generate_mesh(tris: Vec<TriangleData>) -> Mesh {
    let mut all_verts = Vec::<Vec3>::new();
    let mut all_norms = Vec::<Vec3>::new();
    let mut all_uvs = Vec::<Vec2>::new();
    let mut all_indices = Vec::<u32>::new();
    // let mut all_indices_grp = Vec::<[u32; 3]>::new();

    let mut idx: u32 = 0;
    for data in tris.iter() {
        all_verts.extend_from_slice(&data.verts);
        all_norms.extend_from_slice(&data.norm);
        all_uvs.extend_from_slice(&data.uvs);
        all_indices.extend_from_slice(&[idx, idx + 1, idx + 2]);
        // all_indices_grp.push([idx, idx + 1, idx + 2]);
        idx += 3;
    }
    // let collider = Collider::trimesh(all_verts.clone(), all_indices_grp);

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_indices(mesh::Indices::U32(all_indices));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, all_verts);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, all_norms);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, all_uvs);
    mesh
}

impl TileCoord {
    fn geo_bbox(self: &Self) -> GeoBBox {
        let rv = GeoBBox {
            lon_west: (self.x as f64 / 2.0_f64.powi(self.z as i32)) * 360.0 - 180.0,
            lon_east: ((self.x + 1) as f64 / 2.0_f64.powi(self.z as i32)) * 360.0 - 180.0,
            lat_south: (PI - ((self.y + 1) as f64) / 2.0_f64.powi(self.z as i32) * 2.0 * PI)
                .sinh()
                .atan()
                * 180.0
                / PI,
            lat_north: (PI - (self.y as f64) / 2.0_f64.powi(self.z as i32) * 2.0 * PI)
                .sinh()
                .atan()
                * 180.0
                / PI,
        };
        info!("{:?} {:?}", self, rv);
        rv
    }
}

const INIT_TILES_START_LEVEL: u8 = 4;

fn init_tiles() -> Vec<TileCoord> {
    let mut vec = Vec::<TileCoord>::new();
    let z: u8 = INIT_TILES_START_LEVEL;
    for x in 0..2_u64.pow(z as u32) {
        for y in 0..2_u64.pow(z as u32) {
            vec.push(TileCoord { x, y, z });
        }
    }
    vec
}
use bevy::render::mesh::{self, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
/// set up a simple 3D scene
fn setup_load_tasks(runtime: ResMut<TokioTasksRuntime>) {
    for tile in init_tiles() {
        let t2 = tile.clone();
        runtime.spawn_background_task(move |mut ctx| async move {
            let (mesh, image) = get_tile(t2).await;

            let mesh_handle = ctx
                .run_on_main_thread(move |ctx| {
                    let mut meshes = ctx.world.get_resource_mut::<Assets<Mesh>>().unwrap();
                    let mesh_handle = meshes.add(mesh);
                    mesh_handle
                })
                .await;

            let image_handle = ctx
                .run_on_main_thread(move |ctx| {
                    let mut images = ctx.world.get_resource_mut::<Assets<Image>>().unwrap();

                    let image_handle = images.add(image);
                    image_handle
                })
                .await;

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
            ctx.run_on_main_thread(move |ctx| {
                let bundle = PbrBundle {
                    mesh: mesh_handle,
                    material: mat_handle,
                    ..default()
                };
                ctx.world.spawn_empty().insert(bundle);
            })
            .await;
        });
    }
}
fn setup(
    mut commands: Commands,
) {
    // light
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });
    // camera
    let [target, eye, up] = [Vec3::ZERO, Vec3::Z * 10.0, Vec3::Y];
    commands.spawn((
        bevy_trackball::TrackballController::default(),
        bevy_trackball::TrackballCamera::look_at(target, eye, up),
        Camera3dBundle {
            transform: Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
    ));
}
async fn get_tile(tile: TileCoord) -> (Mesh, Image) {
    let mesh = generate_mesh(tile.geo_bbox().to_tris());

    let tile_url = format!(
        "http://localhost:8000/api/tile/arcgis_sat/{}/{}/{}/tile.jpg",
        tile.z, tile.x, tile.y
    );
    let img = reqwest::get(tile_url).await.unwrap().bytes().await.unwrap();
    info!("downlaoded {} bytes", img.len());

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

fn max3(l1: f32, l2: f32, l3: f32) -> f32 {
    if l1 > l2 {
        if l1 > l3 {
            l1
        } else {
            l3
        }
    } else if l2 > l3 {
        l2
    } else {
        l3
    }
}

fn min3(l1: f32, l2: f32, l3: f32) -> f32 {
    if l1 < l2 {
        if l1 < l3 {
            l1
        } else {
            l3
        }
    } else if l2 < l3 {
        l2
    } else {
        l3
    }
}
