use crate::earth_fetch;
use crate::{earth_fetch::WebMercatorTiledPlanet, geo_trig::TileCoord};
use bevy::prelude::*;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub struct ConfigTileServersPlugin {}

impl Plugin for ConfigTileServersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, download_server_configs);
        app.add_systems(Update, update_egui_tile_picker);
    }
}

fn update_egui_tile_picker(
    mut contexts: bevy_egui::EguiContexts,
    mut planets: Query<&mut WebMercatorTiledPlanet>,
    servers: Res<TileServers>,
) {
    for mut planet in planets.iter_mut() {
        let mut current_type = planet.tile_type.clone();
        let ctx = contexts.ctx_mut();
        egui::Window::new(&planet.planet_name)
            .vscroll(true)
            .show(ctx, |ui| {
                for (map_type, server_ids) in servers
                    .list_for_planet_group_by_map_type(&planet.planet_name)
                {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(map_type.to_uppercase())
                                .size(30.0)
                                .underline(),
                        );
                        for server_id in server_ids {
                            let egui_img =
                                servers.get_egui_demo_image(&server_id);
                            ui.horizontal(|ui| {
                                ui.add(egui::widgets::Image::new(
                                    egui::load::SizedTexture::new(
                                        egui_img,
                                        [64.0, 64.0],
                                    ),
                                ));
                                ui.vertical(|ui| {
                                    ui.selectable_value(
                                        &mut current_type,
                                        server_id.clone(),
                                        &server_id,
                                    );
                                });
                            });
                        }
                    });
                }
            });
        if !current_type.eq(&planet.tile_type) {
            info!("change {} to {}", planet.planet_name, current_type);
            planet.tile_type = current_type;
        }
    }
}

#[derive(
    Deserialize, Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub struct TileServerConfig {
    pub planet: String,
    pub map_type: String,
    pub name: String,
    pub comment: String,
    pub url: String,
    pub width: u32,
    pub height: u32,
    pub max_level: u8,
    pub img_type: String,
    pub servers: Option<Vec<String>>,
}

impl TileServerConfig {
    pub fn get_tile_url(&self, tile: TileCoord) -> String {
        format!(
            "http://localhost:8000/api/tile/{}/{}/{}/{}/tile.{}",
            self.name, tile.z, tile.x, tile.y, self.img_type
        )
    }
    pub fn img_type(&self) -> image::ImageFormat {
        match self.img_type.as_str() {
            "jpg" => image::ImageFormat::Jpeg,
            "png" => image::ImageFormat::Png,
            _ => panic!("unknwon img frmat"),
        }
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct TileServers {
    servers: Arc<HashMap<String, TileServerConfig>>,
    demo_images: Arc<HashMap<String, Handle<Image>>>,
    egui_images: Arc<HashMap<String, egui::TextureId>>,
}

impl TileServers {
    pub fn get_demo_image(&self, name: &str) -> Handle<Image> {
        self.demo_images
            .get(name)
            .expect("cannot find demo image")
            .clone_weak()
    }
    pub fn get_egui_demo_image(&self, name: &str) -> egui::TextureId {
        self.egui_images
            .get(name)
            .expect("cannot find demo image")
            .clone()
    }

    pub fn get(&self, name: &str) -> TileServerConfig {
        self.servers
            .get(name)
            .expect("unknwon tileserver name.")
            .clone()
    }
    pub fn list_for_planet(&self, planet: &str) -> Vec<TileServerConfig> {
        let mut v: Vec<_> = self
            .servers
            .values()
            .filter(|k| k.planet.eq(planet))
            .map(|k| k.clone())
            .collect();
        v.sort();
        v
    }

    pub fn list_for_planet_group_by_map_type(
        &self,
        planet: &str,
    ) -> Vec<(String, Vec<String>)> {
        use itertools::Itertools;
        let mut data_grp: Vec<(_, Vec<_>)> = Vec::new();
        for (k, grp) in &self
            .list_for_planet(planet)
            .into_iter()
            .group_by(|e| e.map_type.clone())
        {
            data_grp.push((k, grp.map(|e| e.name.clone()).collect()))
        }
        data_grp.sort_by_key(|v| -(v.1.len() as isize));
        data_grp
    }
}

pub fn download_server_configs(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut egui_context: bevy_egui::EguiContexts,
) {
    use rayon::iter::IntoParallelRefIterator;
    use rayon::iter::ParallelIterator;

    let url = "http://localhost:8000/api/config/tileservers.json";
    let resp = reqwest::blocking::get(url)
        .expect("cannot get tile server config (check if backend up)");
    let mut data: Vec<TileServerConfig> =
        resp.json().expect("server config not valid json");
    data.sort();

    info!("downloaded {} tile server configs", data.len());
    let srv_map =
        HashMap::from_iter(data.into_iter().map(|v| (v.name.clone(), v)));
    let img_map: HashMap<_, _> = srv_map
        .par_iter()
        .map(|(k, v)| {
            let img = reqwest::blocking::get(v.get_tile_url(TileCoord {
                x: 1,
                y: 0,
                z: 1,
            }))
            .expect("cannot download demo img")
            .bytes()
            .expect("why no bytes");
            info!("downloaded demoimg {} size {}", k, img.len());
            let img = earth_fetch::parse_bytes_to_image(img, v.img_type());
            (k.clone(), img)
        })
        .collect();
    let img_map: HashMap<_, _> = img_map
        .into_iter()
        .map(|(k, v)| (k.to_owned(), images.add(v)))
        .collect();
    let egui_img_map = img_map
        .iter()
        .map(|(k, v)| (k.clone(), egui_context.add_image(v.clone())))
        .collect();
    commands.insert_resource(TileServers {
        servers: Arc::new(srv_map),
        demo_images: Arc::new(img_map),
        egui_images: Arc::new(egui_img_map),
    });
}
