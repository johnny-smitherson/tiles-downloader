use crate::config::LINKS_CONFIG;
use anyhow::Context;
use duckdb::arrow::record_batch::RecordBatch;
use duckdb::arrow::util::pretty::pretty_format_batches;
use duckdb::Connection;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use strfmt::strfmt;

const OVERT_LOCATION: &str = "s3://overturemaps-us-west-2/release";
const OVERT_VERSION: &str = "2024-04-16-beta.0";
const OVERT_TABLES: [(&str, &str); 21] = [
    ("admins", "*"),
    ("admins", "administrative_boundary"),
    ("admins", "locality"),
    ("admins", "locality_area"),
    ("base", "*"),
    ("base", "infrastructure"),
    ("base", "land"),
    ("base", "land_use"),
    ("base", "water"),
    ("buildings", "*"),
    ("buildings", "building"),
    ("buildings", "building_part"),
    ("divisions", "*"),
    ("divisions", "boundary"),
    ("divisions", "division"),
    ("divisions", "division_area"),
    ("places", "*"),
    ("places", "place"),
    ("transportation", "*"),
    ("transportation", "connector"),
    ("transportation", "segment"),
];

const INIT_SQL_SETTINGS: &str = "
INSTALL spatial;
INSTALL httpfs;
LOAD spatial;
LOAD httpfs;
SET s3_region='us-west-2';

SET memory_limit = '10GB';
SET threads TO 8;
-- enable printing of a progress bar during long-running queries
SET enable_progress_bar = true;
-- set the default null order to NULLS LAST
SET default_null_order = 'nulls_last';
";

const CREATE_VIEW_TEMPLATE: &str = "
CREATE  VIEW  IF NOT EXISTS {view_name} AS (
    SELECT
        *
    FROM
        read_parquet('{overt_location}/{overt_version}/theme={overt_theme}/type={overt_type}/*', filename=true, hive_partitioning=1)
);
";

const SQL_LIST_VIEWS: &str = "
select table_name from information_schema.tables
where table_catalog = 'db'
and table_schema = 'main'
and table_type = 'VIEW'
";

// "WHERE primary_name IS NOT NULL
// AND bbox.xmin > -84.36
// AND bbox.xmax < -82.42
// AND bbox.ymin > 41.71
// AND bbox.ymax < 43.33;
// "

fn geo_view_name(theme: &str, _type: &str) -> String {
    if _type.eq("*") {
        format!("{}_view", theme)
    } else {
        format!("{}_{}_view", theme, _type)
    }
}

pub async fn geoduck_execute_to_str(sql: &str) -> anyhow::Result<String> {
    let sql = sql.to_string();
    tokio::task::spawn_blocking(move || {
        let conn = get_duck_connection()?;
        let mut s = conn.prepare(&sql)?;
        let v: Vec<_> = s.query_arrow([])?.collect();
        Ok(format!("{}", pretty_format_batches(&v)?))
    })
    .await?
}

fn create_all_views() -> anyhow::Result<()> {
    let conn = get_duck_connection()?;

    for (overt_theme, overt_type) in OVERT_TABLES.iter() {
        let view_name = geo_view_name(overt_theme, overt_type);
        let mut map: HashMap<String, String> = HashMap::with_capacity(10);
        map.insert("overt_theme".to_owned(), overt_theme.to_string());
        map.insert("overt_type".to_owned(), overt_type.to_string());
        map.insert("view_name".to_owned(), view_name.clone());
        map.insert("overt_location".to_owned(), OVERT_LOCATION.to_string());
        map.insert("overt_version".to_owned(), OVERT_VERSION.to_string());

        let sql = strfmt::strfmt(CREATE_VIEW_TEMPLATE, &map)
            .context("failed strfmt on sql")?;
        eprintln!("{}", sql);
        conn.execute_batch(&sql)?;
        eprintln!(
            "duck added geo view '{}' theme={}/type={}",
            view_name.clone(),
            overt_theme,
            overt_type
        );
    }
    Ok(())
}

pub fn get_duck_connection() -> anyhow::Result<duckdb::Connection> {
    Ok(Connection::open(
        &LINKS_CONFIG.tile_location.join("geoduck").join("db.duck"),
    )?)
}

pub fn init_geoduck() -> anyhow::Result<()> {
    std::fs::create_dir_all(&LINKS_CONFIG.tile_location.join("geoduck"))?;

    let conn = get_duck_connection()?;
    eprintln!("duck: connection open");
    conn.execute_batch(INIT_SQL_SETTINGS)?;
    eprintln!("duck: extensions installed");

    // TODO create the views if they don't exist

    Ok(())
}
