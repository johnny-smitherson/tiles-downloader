use anyhow::Result;

use std::{f64, path::Path, str::FromStr, time::Duration};

use osm_tile_downloader::*;
use clap::Arg;

const BBOX_NORTH_ARG: &str = "BBOX_NORTH";
const BBOX_SOUTH_ARG: &str = "BBOX_SOUTH";
const BBOX_WEST_ARG: &str = "BBOX_WEST";
const BBOX_EAST_ARG: &str = "BBOX_EAST";
const OUTPUT_ARG: &str = "OUTPUT";
const PARALLEL_FETCHES_ARG: &str = "PARALLEL_FETCHES";
const REQUEST_RETRIES_ARG: &str = "REQUEST_RETRIES";
const UP_TO_ZOOM_ARG: &str = "UP_TO_ZOOM";
const URL_ARG: &str = "URL";
const TIMEOUT_ARG: &str = "TIMEOUT";

#[tokio::main]
async fn main() -> Result<()> {


    let matches = clap::command!("osm_tile_downloader")
        .bin_name("cargo")
        .arg(
            Arg::new(BBOX_NORTH_ARG)
                .help("Latitude of north bounding box boundary (in degrees)")
                .value_parser(clap::value_parser!(f64))
                .required(true)
                .allow_negative_numbers(true)
                .long("north"),
        )
        .arg(
            Arg::new(BBOX_SOUTH_ARG)
                .help("Latitude of south bounding box boundary (in degrees)")
                .value_parser(clap::value_parser!(f64))
                .required(true)
                .allow_negative_numbers(true)
                .long("south"),
        )
        .arg(
            Arg::new(BBOX_EAST_ARG)
                .help("Longitude of east bounding box boundary (in degrees)")
                .value_parser(clap::value_parser!(f64))
                .allow_negative_numbers(true)
                .required(true)
                .long("east"),
        )
        .arg(
            Arg::new(BBOX_WEST_ARG)
                .help("Longitude of west bounding box boundary (in degrees)")
                .value_parser(clap::value_parser!(f64))
                .allow_negative_numbers(true)
                .required(true)
                .long("west"),
        )
        .arg(
            Arg::new(PARALLEL_FETCHES_ARG)
                .help("The amount of tiles fetched in parallel.")
                .value_parser(clap::value_parser!(u8))
                .default_value("5")
                .long("rate"),
        )
        .arg(
            Arg::new(REQUEST_RETRIES_ARG)
                .help("The amount of times to retry a failed HTTP request.")
                .value_parser(clap::value_parser!(u8))
                .default_value("3")
                .long("retries"),
        )
        .arg(
            Arg::new(TIMEOUT_ARG)
                .help("The timeout (in seconds) for fetching a single tile. Pass 0 for no timeout.")
                .value_parser(clap::value_parser!(u64))
                .default_value("10")
                .long("timeout"),
        )
        .arg(
            Arg::new(UP_TO_ZOOM_ARG)
                .help("The maximum zoom level to fetch")
                .value_parser(clap::value_parser!(u8))
                .default_value("18")
                .long("zoom"),
        )
        .arg(
            Arg::new(OUTPUT_ARG)
                .help("The folder to output the tiles to. May contain format specifiers (and subfolders) to specify how the files will be laid out on disk.")
                .value_parser(clap::value_parser!(String))
                .default_value("output")
                .long("output"),
        )
        .arg(
            Arg::new(URL_ARG)
                .help("The URL with format specifiers `{x}`, `{y}`, `{z}` to fetch the tiles from. Also supports the format specifier `{s}` which is replaced with `a`, `b` or `c` randomly to spread the load between different servers.")
                .required(true)
                .value_parser(clap::value_parser!(String))
                .long("url")
        )
        .get_matches();

    let config = Config {
        bounding_box: BoundingBox::new_deg(
            *matches.get_one::<f64>(BBOX_NORTH_ARG).unwrap(),
            *matches.get_one::<f64>(BBOX_EAST_ARG).unwrap(),
            *matches.get_one::<f64>(BBOX_SOUTH_ARG).unwrap(),
            *matches.get_one::<f64>(BBOX_WEST_ARG).unwrap(),
        ),
        fetch_rate:*matches.get_one::<u8>(PARALLEL_FETCHES_ARG).unwrap(),
        output_folder: Path::new(matches.get_one::<String>(OUTPUT_ARG).unwrap()),
        request_retries_amount: *matches.get_one::<u8>(REQUEST_RETRIES_ARG).unwrap(),
        url: matches.get_one::<String>(URL_ARG).unwrap(),
        timeout: Duration::from_secs(
            *matches.get_one::<u64>(TIMEOUT_ARG).unwrap()
        ),
        zoom_level: *matches.get_one::<u8>(UP_TO_ZOOM_ARG).unwrap(),
    };

    fetch(config).await?;
    Ok(())
}
