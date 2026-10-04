//! US-74: builds the offline place database the name suggestion reads
//! (ADR-0027), on the laptop, and cuts test fixtures out of it. All logic
//! lives in `places::build` and is tested there — this file is a thin shell,
//! the same policy as the other CLIs.
//!
//! Usage:
//!   `places_build build <out.sqlite> <source>...` — needs `osmium`; each
//!   source an OSM extract (`*.osm.pbf`) or Kartverket's register (`*.gml`)
//!   `places_build cut <places.sqlite> <out.sqlite> <track.gpx>...`
//!
//! How the database is refreshed and deployed is in `docs/deployment.md`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use trip_archive::server::places::{build, PlaceWriter};

const USAGE: &str =
    "usage: places_build build <out.sqlite> <extract.osm.pbf|register.gml>...\n       \
                     places_build cut <places.sqlite> <out.sqlite> <track.gpx>...";

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
