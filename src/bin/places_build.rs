//! US-74/US-76: builds the offline place and ground databases the name and
//! activity suggestions read (ADR-0027), on the laptop, and cuts test
//! fixtures out of them. All logic lives in `places::build` and
//! `ground::build` and is tested there — this file is a thin shell, the same
//! policy as the other CLIs.
//!
//! Usage:
//!   `places_build build <out.sqlite> <source>...` — needs `osmium`; each
//!   source an OSM extract (`*.osm.pbf`) or Kartverket's register (`*.gml`)
//!   `places_build cut <places.sqlite> <out.sqlite> <track.gpx>...`
//!   `places_build ground <out.sqlite> <extract.osm.pbf>... <water_polygons.shp>`
//!   — needs `osmium`; the sea from osmdata.openstreetmap.de's split water
//!   polygons in WGS 84
//!   `places_build cut-ground <ground.sqlite> <out.sqlite> <track.gpx>...`
//!
//! How the database is refreshed and deployed is in `docs/deployment.md`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use trip_archive::server::ground::{self, GroundWriter};
use trip_archive::server::places::{build, PlaceWriter};

const USAGE: &str =
    "usage: places_build build <out.sqlite> <extract.osm.pbf|register.gml>...\n       \
                     places_build cut <places.sqlite> <out.sqlite> <track.gpx>...\n       \
                     places_build ground <out.sqlite> <extract.osm.pbf>... <water_polygons.shp>\n       \
                     places_build cut-ground <ground.sqlite> <out.sqlite> <track.gpx>...";

async fn run(args: &[String]) -> anyhow::Result<()> {
    match args {
        [command, out, sources @ ..] if command == "build" && !sources.is_empty() => {
            let mut writer = PlaceWriter::create(Path::new(out)).await?;
            for source in sources {
                let path = Path::new(source);
                let found = if path.extension().is_some_and(|ext| ext == "gml") {
                    build::add_kartverket(&mut writer, path).await?
                } else {
                    build::add_osm_extract(&mut writer, path).await?
                };
                println!("{source}: {found} places");
            }
            writer.finish().await?;
            Ok(())
        }
        [command, source, out, tracks @ ..] if command == "cut" && !tracks.is_empty() => {
            let tracks: Vec<PathBuf> = tracks.iter().map(PathBuf::from).collect();
            let tracks: Vec<&Path> = tracks.iter().map(PathBuf::as_path).collect();
            build::cut_fixture(Path::new(source), Path::new(out), &tracks).await
        }
        [command, out, extracts @ .., sea] if command == "ground" && !extracts.is_empty() => {
            let mut writer = GroundWriter::create(Path::new(out)).await?;
            for extract in extracts {
                let found = ground::build::add_osm_extract(&mut writer, Path::new(extract)).await?;
                println!("{extract}: {found} ways and waters");
            }
            let found = ground::build::add_sea(&mut writer, Path::new(sea)).await?;
            println!("{sea}: {found} pieces of sea");
            writer.finish().await?;
            Ok(())
        }
        [command, source, out, tracks @ ..] if command == "cut-ground" && !tracks.is_empty() => {
            let tracks: Vec<PathBuf> = tracks.iter().map(PathBuf::from).collect();
            let tracks: Vec<&Path> = tracks.iter().map(PathBuf::as_path).collect();
            ground::build::cut_fixture(Path::new(source), Path::new(out), &tracks).await
        }
        _ => anyhow::bail!(USAGE),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}
