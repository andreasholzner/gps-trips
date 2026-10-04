//! Building the ground database on the laptop (`places_build ground`,
//! ADR-0027): the ways and water of OpenStreetMap extracts, through the
//! `osmium` command-line tool, and the sea from the water polygons derived
//! from OSM's coastlines (osmdata.openstreetmap.de, a shapefile).

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context};
use geo::{Coord, Intersects, MultiPolygon, Rect, Simplify};

use super::osm::{ground_from_feature, Ground};
use super::GroundWriter;
use crate::server::gpx::parse_gpx;
use crate::server::osmium;
use crate::server::track_boxes;

/// The OSM objects `osm::ground_from_feature` may keep, as `osmium
/// tags-filter` expressions.
const OSM_FILTERS: [&str; 3] = ["w/highway", "nwr/natural=water", "nwr/waterway=riverbank"];

/// How far the sea's outlines are simplified, in degrees: about 10 m
/// north-south.
const SIMPLIFY_DEGREES: f64 = 0.0001;

/// How far around a fixture's tracks the ground data is cut: past the
/// neighbouring cells a point is read from.
const FIXTURE_REACH_M: f64 = 100.0;

/// Adds the ways and water of the OSM extract `pbf` to `writer`, and the
/// box it covers; how many ways and bodies of water it found.
pub async fn add_osm_extract(writer: &mut GroundWriter, pbf: &Path) -> anyhow::Result<u64> {
    writer.add_coverage(extract_box(pbf)?).await?;

    let mut features = osmium::export(pbf, &OSM_FILTERS, "linestring,polygon")?;
    let mut found = 0;
    for feature in features.by_ref() {
        match ground_from_feature(&feature?) {
            Some(Ground::Way(way, coords)) => writer.add_way(way, &coords),
            Some(Ground::Water(area)) => {
                for polygon in area {
                    writer.add_water(polygon).await?;
                }
            }
            None => continue,
        }
        found += 1;
    }
    features
        .finish()
        .with_context(|| format!("reading {}", pbf.display()))?;
    // One extract's ways at a time in memory, not every region's.
    writer.flush_tiles().await?;
    Ok(found)
}

/// The bounding box an extract's header gives.
fn extract_box(pbf: &Path) -> anyhow::Result<Rect> {
    let output = Command::new("osmium")
        .args(["fileinfo", "-g", "header.boxes"])
        .arg(pbf)
        .output()
        .context("running osmium fileinfo")?;
    let text = String::from_utf8_lossy(&output.stdout);
    parse_box(&text).with_context(|| format!("{} has no bounding box: {text}", pbf.display()))
}

/// `(5.1,57.8,31.4,71.3)` as osmium prints a box.
fn parse_box(text: &str) -> Option<Rect> {
    let inner = text.trim().strip_prefix('(')?.split(')').next()?;
    let n: Vec<f64> = inner
        .split(',')
        .map(|v| v.trim().parse())
        .collect::<Result<_, _>>()
        .ok()?;
    let [x0, y0, x1, y1] = n[..] else {
        return None;
    };
    Some(Rect::new(Coord { x: x0, y: y0 }, Coord { x: x1, y: y1 }))
}

/// Adds the sea from the water polygons at `shp`, where the extracts
/// already added cover it; how many polygons it took.
pub async fn add_sea(writer: &mut GroundWriter, shp: &Path) -> anyhow::Result<u64> {
    let coverage = writer.coverage().to_vec();
    if coverage.is_empty() {
        bail!("add the OSM extracts before the sea: only the sea they cover is kept");
    }
    let mut reader = shapefile::ShapeReader::from_path(shp)
        .with_context(|| format!("opening {}", shp.display()))?;
    let mut found = 0;
    for shape in reader.iter_shapes_as::<shapefile::Polygon>() {
        let shape = shape.with_context(|| format!("reading {}", shp.display()))?;
        let bbox = shape.bbox();
        let rect = Rect::new(
            Coord {
                x: bbox.min.x,
                y: bbox.min.y,
            },
            Coord {
                x: bbox.max.x,
                y: bbox.max.y,
            },
        );
        if !coverage.iter().any(|covered| covered.intersects(&rect)) {
            continue;
        }
        let area = MultiPolygon::try_from(shape)?;
        for polygon in area.simplify(&SIMPLIFY_DEGREES) {
            writer.add_water(polygon).await?;
        }
        found += 1;
    }
    Ok(found)
}

/// Writes to `out` what the ground database at `source` knows around each
/// of the `gpx` tracks.
pub async fn cut_fixture(source: &Path, out: &Path, gpx: &[&Path]) -> anyhow::Result<()> {
    let mut writer = GroundWriter::create(out).await?;
    for path in gpx {
        let raw = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let track = parse_gpx(&raw).with_context(|| format!("parsing {}", path.display()))?;
        let coords: Vec<Coord> = track
            .points
            .iter()
            .map(|p| Coord { x: p.lon, y: p.lat })
            .collect();
        writer
            .copy_within(source, &track_boxes::around(&coords, FIXTURE_REACH_M))
            .await?;
    }
    writer.finish().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::ground::{GroundDb, Surface};
    use shapefile::{Point, Polygon, PolygonRing};

    /// A square `side` degrees across, its south-west corner at (`x`, `y`).
    fn square(x: f64, y: f64, side: f64) -> Polygon {
        Polygon::new(PolygonRing::Outer(vec![
            Point::new(x, y),
            Point::new(x, y + side),
            Point::new(x + side, y + side),
            Point::new(x + side, y),
            Point::new(x, y),
        ]))
    }

    #[tokio::test]
    async fn us76_the_sea_is_kept_where_the_extracts_cover_it() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let shp = dir.path().join("water_polygons.shp");
        let mut sea = shapefile::ShapeWriter::from_path(&shp).expect("a shapefile");
        sea.write_shape(&square(8.0, 60.0, 0.1)).expect("written");
        sea.write_shape(&square(100.0, 10.0, 0.1)).expect("written");
        drop(sea);
        let path = dir.path().join("ground.sqlite");
        let mut writer = GroundWriter::create(&path).await.expect("a new database");
        writer
            .add_coverage(Rect::new(
                Coord { x: 7.0, y: 59.0 },
                Coord { x: 9.0, y: 61.0 },
            ))
            .await
            .expect("coverage");

        let kept = add_sea(&mut writer, &shp).await.expect("read");
        writer.finish().await.expect("finished");

        assert_eq!(kept, 1);
        let db = GroundDb::open(&path).await.expect("opens").expect("exists");
        let at_sea = Coord { x: 8.05, y: 60.05 };
        let ashore = Coord { x: 8.2, y: 60.05 };
        let data = db.around(&[at_sea, ashore]).await.expect("read");
        assert_eq!(data.surface_at(at_sea), Surface::Water);
        assert_eq!(data.surface_at(ashore), Surface::OffRoad);
    }

    #[tokio::test]
    async fn us76_the_sea_needs_the_extracts_first() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let mut writer = GroundWriter::create(&dir.path().join("ground.sqlite"))
            .await
            .expect("a new database");

        let refused = add_sea(&mut writer, &dir.path().join("none.shp")).await;

        assert!(refused.is_err());
    }

    #[test]
    fn us76_an_extract_s_box_is_read_as_osmium_prints_it() {
        let rect = parse_box("(4.0,57.75,31.5,71.5)\n").expect("a box");
        assert_eq!(rect.min(), Coord { x: 4.0, y: 57.75 });
        assert_eq!(rect.max(), Coord { x: 31.5, y: 71.5 });
        assert_eq!(parse_box(""), None);
        assert_eq!(parse_box("(1,2,3)"), None);
    }
}
