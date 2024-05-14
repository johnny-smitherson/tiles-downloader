#![allow(clippy::assigning_clones)]
#![allow(clippy::needless_borrows_for_generic_args)]

pub(crate) mod config;
pub(crate) mod download_everything;
pub(crate) mod download_geoduck;
pub(crate) mod download_geosearch;
pub(crate) mod download_tile;
pub(crate) mod fetch;
pub(crate) mod geo_trig;
pub(crate) mod http_api;
pub(crate) mod http_pages;
pub(crate) mod proxy_manager;
pub(crate) mod rocket_anyhow;
pub(crate) mod stat_counter;

#[macro_use]
extern crate rocket;

extern crate overt_geoduck;

use rocket_dyn_templates::Template;

use config::init_database;

// use rocket::form::Form;

// #[derive(FromForm)]
// struct GeoDuckReplRequest {
//     sql_query: String,
// }

// #[post("/api/geoduck/repl", data = "<form>")]
// async fn geoduck_repl_api(
//     form: Form<GeoDuckReplRequest>,
// ) -> rocket_anyhow::Result<String> {

//     Ok(overt_geoduck::geoduck_execute_to_str(&form.sql_query).await?)
// }

// #[get("/geoduck/repl")]
// fn geoduck_repl() -> rocket_anyhow::Result<Template> {
//     Ok(Template::render("geoduck", context! {}))
// }

#[rocket::main]
async fn main() -> rocket_anyhow::Result<()> {
    init_database().await?;
    // overt_geo_duck::init_geoduck()?;
    // check we can run the manager once
    // let _fetch_manager = tokio::spawn(fetch::fetch_loop());
    let _proxy_manager = tokio::spawn(proxy_manager::proxy_manager_loop());

    let config = rocket::Config {
        log_level: rocket::config::LogLevel::Critical,
        workers: 16,
        ..Default::default()
    };
    let _rocket = rocket::build()
    .configure(config)
        .mount("/", http_api::get_api_routes())
        .mount("/", http_pages::get_page_routes())
        .attach(Template::fairing())
        .launch()
        .await?;

    eprintln!("aborting worker loops...");
    _proxy_manager.abort();
    // _fetch_manager.abort();
    eprintln!("clean exit done.");

    Ok(())
}
