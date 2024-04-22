use anyhow::Context;
use duckdb::arrow::record_batch::RecordBatch;
use duckdb::arrow::util::pretty::pretty_format_batches;
use duckdb::Connection;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

const OVERT_LOCATION: &str = "s3://overturemaps-us-west-2/release";
const OVERT_VERSION: &str = "2024-04-16-beta.0";
pub const OVERT_TABLES: [(&str, &str); 21] = [
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OvertDataType {
    pub theme: String,
    pub _type: String,
}

const SQL_INIT_SETTINGS: &str = "
INSTALL spatial;
INSTALL httpfs;
LOAD spatial;
LOAD httpfs;
SET s3_region='us-west-2';
SET memory_limit = '1GB';
SET threads TO 8;
SET enable_progress_bar = true;
";

const SQL_CREATE_VIEW: &str = "
CREATE  VIEW  IF NOT EXISTS {view_name} AS (
    SELECT
        *
    FROM
        read_parquet('{overt_location}/{overt_version}/theme={overt_theme}/type={overt_type}/*', filename=true, hive_partitioning=1)
);
";

const SQL_SELECT_AS_JSON: &str = "
COPY(
    SELECT *, ST_GeomFromWKB(geometry) as geometry
    FROM {view_name}
    WHERE  bbox.xmin > {xmin}  AND bbox.xmax < {xmax} AND bbox.ymin > {ymin} AND bbox.ymax < {ymax}
) TO '{json_file_path}'
WITH (FORMAT GDAL, DRIVER 'GeoJSON');
";

// "WHERE primary_name IS NOT NULL
// AND bbox.xmin > -84.36
// AND bbox.xmax < -82.42
// AND bbox.ymin > 41.71
// AND bbox.ymax < 43.33;
// "

impl OvertDataType {
    fn new(theme: &str, _type: &str) -> Self {
        Self {
            theme: theme.to_string(),
            _type: _type.to_string(),
        }
    }
    fn view_name(&self) -> String {
        if self._type.eq("*") {
            format!("{}_s3_view", self.theme)
        } else {
            format!("{}_{}_s3_view", self.theme, self._type)
        }
    }
    fn sql_create_view(&self) -> String {
        let view_name = self.view_name();
        let mut map: HashMap<String, String> = HashMap::with_capacity(10);
        map.insert("overt_theme".to_owned(), self.theme.to_string());
        map.insert("overt_type".to_owned(), self._type.to_string());
        map.insert("view_name".to_owned(), view_name.clone());
        map.insert("overt_location".to_owned(), OVERT_LOCATION.to_string());
        map.insert("overt_version".to_owned(), OVERT_VERSION.to_string());

        let sql =
            strfmt::strfmt(SQL_CREATE_VIEW, &map).expect("sql_create_view: failed strfmt on sql");
        sql
    }
    fn sql_select_to_geojson(
        &self,
        output_json_path: &str,
        xmin: f64,
        xmax: f64,
        ymin: f64,
        ymax: f64,
    ) -> String {
        let view_name = self.view_name();
        let mut map: HashMap<String, String> = HashMap::with_capacity(10);
        map.insert("view_name".to_owned(), view_name.clone());
        map.insert("json_file_path".to_owned(), output_json_path.to_string());
        map.insert("xmin".to_owned(), xmin.to_string());
        map.insert("xmax".to_owned(), xmax.to_string());
        map.insert("ymin".to_owned(), ymin.to_string());
        map.insert("ymax".to_owned(), ymax.to_string());

        let sql = strfmt::strfmt(SQL_SELECT_AS_JSON, &map)
            .expect("sql_select_to_geojson: failed strfmt on sql");
        sql
    }
}

pub fn download_geojson(
    theme: &str,
    _type: &str,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
    geojson_out: &Path,
) -> anyhow::Result<geojson::FeatureCollection> {
    let geojson_out = std::fs::canonicalize(geojson_out)?;
    let geojson_out = geojson_out
        .to_str()
        .context("cannot transform path to string.")?;
    let dt = OvertDataType::new(theme, _type);
    if !OVERT_TABLES.contains(&(theme, _type)) {
        anyhow::bail!("Data Type '{:?}' does not exist, see OVERT_TABLES", &dt);
    }
    let conn = get_duck_connection()?;
    eprintln!("geoduck: creating view for {:?}", dt);
    conn.execute_batch(&dt.sql_create_view())?;
    eprintln!(
        "geoduck: running SQL query for {:?} xmin={} xmax={} ymin={} ymax={}",
        dt, xmin, xmax, ymin, ymax
    );
    conn.execute_batch(&dt.sql_select_to_geojson(geojson_out, xmin, xmax, ymin, ymax))?;
    eprintln!("geoduck: finished downloading file {}", geojson_out);
    if !std::path::PathBuf::from(geojson_out).exists() {
        anyhow::bail!(
            "duck did not dump any json file for {:?} at {}",
            &dt,
            geojson_out
        );
    }
    let fcol: geojson::FeatureCollection = {
        let file = std::fs::File::open(geojson_out).expect("cannot open json file");
        serde_json::from_reader(file)?
    }  ;
    eprintln!("geoduck: from {:?} parsed {} features", &dt, fcol.features.len());
    Ok(fcol)
}

fn get_duck_connection() -> anyhow::Result<duckdb::Connection> {
    let conn = Connection::open_in_memory()?;
    eprintln!("geoduck: initializing");
    conn.execute_batch(SQL_INIT_SETTINGS)?;
    Ok(conn)
}
