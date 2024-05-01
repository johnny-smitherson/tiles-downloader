use std::f64::consts::PI;

use bevy::prelude::*;
use bevy::render::mesh::{self, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;

#[derive(Reflect, Debug, Clone, Copy)]
pub struct TileCoord {
    pub x: u64,
    pub y: u64,
    pub z: u8,
}

#[derive(Reflect, Debug, Clone)]
pub struct GeoBBox {
    lon_west: f64,
    lat_south: f64,
    lon_east: f64,
    lat_north: f64,
}

impl GeoBBox {
    pub fn to_tris(&self) -> Vec<TriangleData> {
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

#[derive(Reflect, Debug, Clone)]
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
        // let mut rng = rand::thread_rng();

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
            max_edge_len: crate::util::max3(l1, l2, l3),
            min_edge_len: crate::util::min3(l1, l2, l3),
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
    pub fn geo_bbox(&self) -> GeoBBox {
        let rv = GeoBBox {
            lon_west: (self.x as f64 / 2.0_f64.powi(self.z as i32)) * 360.0
                - 180.0,
            lon_east: ((self.x + 1) as f64 / 2.0_f64.powi(self.z as i32))
                * 360.0
                - 180.0,
            lat_south: (PI
                - ((self.y + 1) as f64) / 2.0_f64.powi(self.z as i32)
                    * 2.0
                    * PI)
                .sinh()
                .atan()
                * 180.0
                / PI,
            lat_north: (PI
                - (self.y as f64) / 2.0_f64.powi(self.z as i32) * 2.0 * PI)
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

pub fn init_tiles() -> Vec<TileCoord> {
    let mut vec = Vec::<TileCoord>::new();
    let z: u8 = INIT_TILES_START_LEVEL;
    for x in 0..2_u64.pow(z as u32) {
        for y in 0..2_u64.pow(z as u32) {
            vec.push(TileCoord { x, y, z });
        }
    }
    vec
}
