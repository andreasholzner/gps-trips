//! Building the place database on the laptop (`places_build`, ADR-0027):
//! from OpenStreetMap extracts, filtered and turned into GeoJSON by the
//! `osmium` command-line tool, and cutting test fixtures out of the result.

use std::io::BufReader;
use std::path::Path;

use anyhow::Context;

use super::kartverket::Register;
use super::osm::place_from_feature;
use super::PlaceWriter;
use crate::server::gpx::parse_gpx;
use crate::server::name_suggestion::lookup_boxes;
use crate::server::osmium;

/// The OSM objects `osm::place_from_feature` may keep, as `osmium
/// tags-filter` expressions.
const OSM_FILTERS: [&str; 7] = [
    "nwr/place=city,town,village,hamlet,isolated_dwelling,farm,locality",
    "n/natural=peak,volcano,saddle",
    "n/mountain_pass=yes",
    "nwr/tourism=alpine_hut,wilderness_hut,camp_site",
    "nwr/natural=water",
    "nwr/natural=bay",
    "nwr/natural=glacier",
];

/// Adds the places of the OSM extract `pbf` to `writer`; how many it found.
pub async fn add_osm_extract(writer: &mut PlaceWriter, pbf: &Path) -> anyhow::Result<u64> {
    let mut features = osmium::export(pbf, &OSM_FILTERS, "point,polygon")?;
    let mut found = 0;
    for feature in features.by_ref() {
        if let Some((place, id)) = place_from_feature(&feature?) {
            writer.insert(&place, &id).await?;
            found += 1;
        }
    }
    features
        .finish()
        .with_context(|| format!("reading {}", pbf.display()))?;
    Ok(found)
}

/// Adds the places of Kartverket's register, from its GML download at
/// `gml` (EPSG:4258), to `writer`; how many it found.
pub async fn add_kartverket(writer: &mut PlaceWriter, gml: &Path) -> anyhow::Result<u64> {
    let file = std::fs::File::open(gml).with_context(|| format!("opening {}", gml.display()))?;
    let mut found = 0;
    for read in Register::new(BufReader::new(file)) {
        let (place, number) = read.with_context(|| format!("reading {}", gml.display()))?;
        writer.insert(&place, &number).await?;
        found += 1;
    }
    Ok(found)
}

/// Writes to `out` the places of the database at `source` around each of
/// the `gpx` tracks: as far around as any place reaches, so the fixture
/// holds what the suggestion would look up in the real database.
pub async fn cut_fixture(source: &Path, out: &Path, gpx: &[&Path]) -> anyhow::Result<()> {
    let mut writer = PlaceWriter::create(out).await?;
    for path in gpx {
        let raw = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let track = parse_gpx(&raw).with_context(|| format!("parsing {}", path.display()))?;
        let coords: Vec<geo::Coord> = track
            .points
            .iter()
            .map(|p| geo::Coord { x: p.lon, y: p.lat })
            .collect();
        writer.copy_within(source, &lookup_boxes(&coords)).await?;
    }
    writer.finish().await?;
    Ok(())
}
