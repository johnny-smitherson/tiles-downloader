use anyhow::Context;
// use duckdb::arrow::record_batch::RecordBatch;
// use duckdb::arrow::util::pretty::pretty_format_batches;
use duckdb::Connection;
use rand::Rng;
use std::collections::HashMap;
use std::os::windows::fs::MetadataExt;
use std::path::Path;
// use std::path::PathBuf;

const OVERT_LOCATION: &str = "s3://overturemaps-us-west-2/release";
const OVERT_VERSION: &str = "2024-04-16-beta.0";
pub const OVERT_TABLES: [(&str, &str); 15] = [
    // ("admins", "*"),
    ("admins", "administrative_boundary"),
    ("admins", "locality"),
    ("admins", "locality_area"),
    // ("base", "*"),
    ("base", "infrastructure"),
    ("base", "land"),
    ("base", "land_use"),
    ("base", "water"),
    // ("buildings", "*"),
    ("buildings", "building"),
    ("buildings", "building_part"),
    // ("divisions", "*"),
    ("divisions", "boundary"),
    ("divisions", "division"),
    ("divisions", "division_area"),
    // ("places", "*"),
    ("places", "place"),
    // ("transportation", "*"),
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
SET memory_limit = '1900MB';
SET threads TO 4;
SET enable_progress_bar = true;
SET temp_directory = '{temp_directory}';
";

const SQL_CREATE_VIEW_FROM_S3: &str = "
CREATE  VIEW  IF NOT EXISTS {view_name} AS (
    SELECT
        *
    FROM
        read_parquet('{overt_location}/{overt_version}/theme={overt_theme}/type={overt_type}/*', filename=true, hive_partitioning=1)
);
";

const SQL_CREATE_VIEW_FROM_DISK: &str = "
CREATE  VIEW  IF NOT EXISTS {view_name} AS (
    SELECT
        *
    FROM
        read_parquet('{file_path}')
);
";

const SQL_SELECT_AS_JSON: &str = "
COPY(
    SELECT *
    FROM {view_name}
    WHERE  bbox.xmin > {xmin}  AND bbox.xmax < {xmax} AND bbox.ymin > {ymin} AND bbox.ymax < {ymax}
) TO '{file_path}'
WITH (FORMAT PARQUET, COMPRESSION ZSTD);
";
// WITH (FORMAT GDAL, DRIVER 'GeoJSON');

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
    fn sql_create_view_from_web(&self) -> String {
        let view_name = self.view_name();
        let mut map: HashMap<String, String> = HashMap::with_capacity(10);
        map.insert("overt_theme".to_owned(), self.theme.to_string());
        map.insert("overt_type".to_owned(), self._type.to_string());
        map.insert("view_name".to_owned(), view_name.clone());
        map.insert("overt_location".to_owned(), OVERT_LOCATION.to_string());
        map.insert("overt_version".to_owned(), OVERT_VERSION.to_string());

        let sql =
            strfmt::strfmt(SQL_CREATE_VIEW_FROM_S3, &map).expect("sql_create_view: failed strfmt on sql");
        sql
    }
    fn sql_create_view_from_disk(view_name: &str, path: &str) -> String {
        let mut map: HashMap<String, String> = HashMap::with_capacity(10);
        map.insert("file_path".to_owned(), path.to_string());
        map.insert("view_name".to_owned(), view_name.to_string());

        let sql =
            strfmt::strfmt(SQL_CREATE_VIEW_FROM_DISK, &map).expect("sql_create_view: failed strfmt on sql");
        sql
    }
    fn sql_copy_to_parquet(
        view_name: &str,
        output_file_path: &str,
        xmin: f64,
        xmax: f64,
        ymin: f64,
        ymax: f64,
    ) -> String {
        let mut map: HashMap<String, String> = HashMap::with_capacity(10);
        map.insert("view_name".to_owned(), view_name.to_string());
        map.insert("file_path".to_owned(), output_file_path.to_string());
        map.insert("xmin".to_owned(), xmin.to_string());
        map.insert("xmax".to_owned(), xmax.to_string());
        map.insert("ymin".to_owned(), ymin.to_string());
        map.insert("ymax".to_owned(), ymax.to_string());

        let sql = strfmt::strfmt(SQL_SELECT_AS_JSON, &map)
            .expect("sql_select_to_geojson: failed strfmt on sql");
        sql
    }
}

pub fn download_geoparquet(
    theme: &str,
    _type: &str,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
    parquet_out: &Path,
) -> anyhow::Result<usize> {
    eprintln!("downloading {}/{} xmin={} xmax={} ymin={} ymax={}", theme, _type, xmin, xmax, ymin, ymax);
    // let geojson_out = std::fs::canonicalize(geojson_out)?;
    let parquet_out = parquet_out
        .to_str()
        .context("cannot transform path to string.")?;
    let dt = OvertDataType::new(theme, _type);
    if !OVERT_TABLES.contains(&(theme, _type)) {
        anyhow::bail!("Data Type '{:?}' does not exist, see OVERT_TABLES", &dt);
    }
    let conn = get_duck_connection()?;
    let sql_create = dt.sql_create_view_from_web();
    eprintln!("geoduck: creating online view for {:?} \n | sql = {}", dt, sql_create);
    conn.execute_batch(&sql_create)?;

    let sql_select_to_json = OvertDataType::sql_copy_to_parquet(dt.view_name().as_str(), parquet_out, xmin, xmax, ymin, ymax);
    eprintln!(
        "geoduck: running SQL query for {:?} xmin={} xmax={} ymin={} ymax={} \n | sql = {}",
        dt, xmin, xmax, ymin, ymax, &sql_select_to_json
    );
    conn.execute_batch(&sql_select_to_json)?;
    eprintln!("geoduck: finished downloading file {}", parquet_out);
    if !std::path::PathBuf::from(parquet_out).exists() {
        anyhow::bail!(
            "duck did not dump any file for {:?} at {}",
            &dt,
            parquet_out
        );
    }

    Ok(std::fs::metadata(&parquet_out)?.file_size() as usize)
}

pub fn crop_geoparquet(
    parquet_in: &Path,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
    parquet_out: &Path,
) -> anyhow::Result<usize> {
    eprintln!("cropping {:?} xmin={} xmax={} ymin={} ymax={}", parquet_in, xmin, xmax, ymin, ymax);
    // let geojson_out = std::fs::canonicalize(geojson_out)?;
    let parquet_out = parquet_out
        .to_str()
        .context("cannot transform path to string.")?;
    let parquet_in = parquet_in
    .to_str()
    .context("cannot transform path to string.")?;

    let conn = get_duck_connection()?;
    let sql_create = OvertDataType::sql_create_view_from_disk("crop_view", parquet_in);
    eprintln!("geoduck: creating offline view for {:?} \n | sql = {}", parquet_in, sql_create);
    conn.execute_batch(&sql_create)?;

    let sql_select_to_json = OvertDataType::sql_copy_to_parquet("crop_view", parquet_out, xmin, xmax, ymin, ymax);
    eprintln!(
        "geoduck: running SQL query for {:?} xmin={} xmax={} ymin={} ymax={} \n | sql = {}",
        parquet_in, xmin, xmax, ymin, ymax, &sql_select_to_json
    );
    conn.execute_batch(&sql_select_to_json)?;
    eprintln!("geoduck: finished cropping into file {}", parquet_out);
    if !std::path::PathBuf::from(parquet_out).exists() {
        anyhow::bail!(
            "duck did not dump any file for {:?} at {}",
            parquet_in,
            parquet_out
        );
    }

    Ok(std::fs::metadata(&parquet_out)?.file_size() as usize)
}

fn get_duck_connection() -> anyhow::Result<duckdb::Connection> {
    let conn = Connection::open_in_memory()?;
    eprintln!("geoduck: initializing");
    
    let tmp_dir = format!(".tmp/{}", rand::thread_rng().gen::<u128>()) ;
    let mut map: HashMap<String, String> = HashMap::with_capacity(1);

    map.insert("temp_directory".to_owned(), tmp_dir.clone());
    std::fs::create_dir_all(tmp_dir.clone())?;

    let sql = strfmt::strfmt(SQL_INIT_SETTINGS, &map)
    .expect("sql: failed strfmt on sql");


    conn.execute_batch(&sql)?;
    Ok(conn)
}
