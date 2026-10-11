//! Reading a guide book's GPX file (US-86): every track and every waypoint
//! in it, and what the file says about itself for the collection's suggested
//! name and description.
//!
//! A guide book's timestamps, where a file has any, are not when anyone
//! went, so they are dropped here: nothing downstream can compute a date,
//! duration or speed from a collection track — nor from its own GPX, which
//! each track keeps for its download.

use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::server::error::ImportError;
use crate::server::gpx::TrackPoint;

/// A guide book's file, read.
#[derive(Debug)]
pub struct ParsedCollection {
    pub name: Option<String>,
    pub description: Option<String>,
    /// The categories the waypoints carry (Garmin's extension), each once,
    /// in the order the file first names them.
    pub categories: Vec<String>,
    /// In file order; tracks without a single point are left out.
    pub tracks: Vec<ParsedCollectionTrack>,
    pub waypoints: Vec<ParsedWaypoint>,
}

#[derive(Debug)]
pub struct ParsedCollectionTrack {
    /// 1-based, counting every track in the file — those left out too.
    pub position: usize,
    pub name: Option<String>,
    /// Without times.
    pub points: Vec<TrackPoint>,
    /// This track alone as GPX, without times — with the file's waypoints
    /// when it is the file's only track.
    pub gpx: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub struct ParsedWaypoint {
    pub name: Option<String>,
    /// The waypoint's `<desc>`, else its `<cmt>`.
    pub description: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    /// The waypoint's `<sym>`, as the file names it.
    pub symbol: Option<String>,
}

/// Read every track and waypoint of a guide book's file. Refused when it is
/// not GPX, or holds neither a track nor a waypoint.
pub fn parse_collection(data: &[u8]) -> Result<ParsedCollection, ImportError> {
    let gpx: gpx::Gpx =
        gpx::read(std::io::Cursor::new(data)).map_err(|e| ImportError::Parse(e.to_string()))?;

    let waypoints: Vec<ParsedWaypoint> = gpx
        .waypoints
        .iter()
        .map(|wp| ParsedWaypoint {
            name: non_blank(wp.name.as_deref()),
            description: non_blank(wp.description.as_deref())
                .or_else(|| non_blank(wp.comment.as_deref())),
            lat: wp.point().y(),
            lon: wp.point().x(),
            ele: wp.elevation,
            symbol: non_blank(wp.symbol.as_deref()),
        })
        .collect();

    let drawable: Vec<(usize, &gpx::Track)> = gpx
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| track.segments.iter().any(|seg| !seg.points.is_empty()))
        .collect();
    let alone = drawable.len() == 1;
    let tracks = drawable
        .into_iter()
        .map(|(index, track)| {
            let shared_waypoints = if alone { &gpx.waypoints[..] } else { &[] };
            Ok(ParsedCollectionTrack {
                position: index + 1,
                name: non_blank(track.name.as_deref()),
                points: untimed_points(track),
                gpx: track_gpx(track, shared_waypoints)?,
            })
        })
        .collect::<Result<Vec<_>, ImportError>>()?;

    if tracks.is_empty() && waypoints.is_empty() {
        return Err(ImportError::Empty);
    }

    let metadata = gpx.metadata.as_ref();
    Ok(ParsedCollection {
        name: non_blank(metadata.and_then(|m| m.name.as_deref())),
        description: non_blank(metadata.and_then(|m| m.description.as_deref())),
        categories: categories(data),
        tracks,
        waypoints,
    })
}

impl ParsedCollection {
    /// What the collection's name starts as: the file's own, else its file
    /// name without the extension.
    pub fn suggested_name(&self, file_name: &str) -> String {
        self.name.clone().unwrap_or_else(|| {
            Path::new(file_name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .trim()
                .to_string()
        })
    }

    /// What the description starts as: the file's own, then the categories
    /// its waypoints carry.
    pub fn suggested_description(&self) -> String {
        let categories = self.categories.join(", ");
        [self.description.as_deref(), Some(categories.as_str())]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// The track a waypoint belongs to as well as to its collection: the
    /// file's only track, if it has exactly one.
    pub fn waypoint_track(&self) -> Option<usize> {
        (self.tracks.len() == 1).then_some(0)
    }
}

/// A collection track's name: the file's, else the collection's and the
/// track's position in the file.
pub fn track_name(own: Option<&str>, collection: &str, position: usize) -> String {
    own.map_or_else(|| format!("{collection} {position}"), str::to_string)
}

fn non_blank(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn untimed_points(track: &gpx::Track) -> Vec<TrackPoint> {
    track
        .segments
        .iter()
        .flat_map(|seg| &seg.points)
        .map(|wp| TrackPoint {
            lat: wp.point().y(),
            lon: wp.point().x(),
            ele: wp.elevation,
            time: None,
        })
        .collect()
}

fn track_gpx(track: &gpx::Track, waypoints: &[gpx::Waypoint]) -> Result<Vec<u8>, ImportError> {
    let untimed = |point: &gpx::Waypoint| {
        let mut point = point.clone();
        point.time = None;
        point
    };
    let mut track = track.clone();
    for segment in &mut track.segments {
        segment.points = segment.points.iter().map(untimed).collect();
    }
    let single = gpx::Gpx {
        version: gpx::GpxVersion::Gpx11,
        creator: Some("trip-archive".to_string()),
        waypoints: waypoints.iter().map(untimed).collect(),
        tracks: vec![track],
        ..Default::default()
    };
    let mut bytes = Vec::new();
    gpx::write(&single, &mut bytes).map_err(|e| ImportError::Parse(e.to_string()))?;
    Ok(bytes)
}

/// The text of every element named `Category` inside a waypoint, whatever
/// its namespace — Garmin writes the same list under two, and tracks carry
/// categories of their own, which are not the book's. The `gpx` crate does
/// not read extensions, so this is a pass of its own.
fn categories(data: &[u8]) -> Vec<String> {
    let mut reader = Reader::from_reader(data);
    let mut buf = Vec::new();
    let mut found: Vec<String> = Vec::new();
    let mut in_waypoint = false;
    let mut inside = false;
    let mut text = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(start)) if start.local_name().as_ref() == "wpt" => in_waypoint = true,
            Ok(Event::End(end)) if end.local_name().as_ref() == "wpt" => in_waypoint = false,
            Ok(Event::Start(start)) if in_waypoint && start.local_name().as_ref() == "Category" => {
                inside = true;
                text.clear();
            }
            Ok(Event::Text(t)) if inside => text.push_str(&t.xml10_content()),
            Ok(Event::GeneralRef(r)) if inside => match r.resolve_char_ref() {
                // `&#252;` and `&#xFC;`: a letter written by its number.
                Ok(Some(letter)) => text.push(letter),
                _ => text.push_str(match r.as_ref() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => "",
                }),
            },
            Ok(Event::End(end)) if inside && end.local_name().as_ref() == "Category" => {
                inside = false;
                let category = text.trim();
                if !category.is_empty() && !found.iter().any(|known| known == category) {
                    found.push(category.to_string());
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
        buf.clear();
    }
    found
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const GUIDE_BOOK: &[u8] = include_bytes!("../../tests/fixtures/guide_book.gpx");
    const GUIDE_SINGLE: &[u8] = include_bytes!("../../tests/fixtures/guide_single.gpx");
    const WAYPOINTS_ONLY: &[u8] = include_bytes!("../../tests/fixtures/waypoints_only.gpx");
    const NO_TRACKS: &[u8] = include_bytes!("../../tests/fixtures/no_tracks.gpx");

    #[test]
    fn every_track_and_waypoint_of_the_file_is_read_in_file_order() {
        let parsed = parse_collection(GUIDE_BOOK).unwrap();

        let names: Vec<Option<&str>> = parsed.tracks.iter().map(|t| t.name.as_deref()).collect();
        assert_eq!(
            names,
            [Some("Stage 1: Splügen"), None, Some("Stage 3: Albula")]
        );
        // A track's segments are one line.
        assert_eq!(parsed.tracks[1].points.len(), 4);
        assert_eq!(parsed.waypoints.len(), 2);
        assert_eq!(
            parsed.waypoints[0],
            ParsedWaypoint {
                name: Some("San Bernardino Pass".to_string()),
                description: Some("Top of the first climb".to_string()),
                lat: 46.4958,
                lon: 9.1705,
                ele: Some(2065.0),
                symbol: Some("Triangle, Blue".to_string()),
            }
        );
    }

    #[test]
    fn us95_a_waypoint_keeps_its_symbol_as_the_file_names_it() {
        let gpx = br#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1">
  <wpt lat="46.5" lon="9.2"><sym>flag,red</sym></wpt>
  <wpt lat="46.6" lon="9.3"><sym>  </sym></wpt>
  <wpt lat="46.7" lon="9.4"></wpt>
</gpx>"#;

        let symbols: Vec<Option<String>> = parse_collection(gpx)
            .unwrap()
            .waypoints
            .into_iter()
            .map(|wp| wp.symbol)
            .collect();

        assert_eq!(symbols, [Some("flag,red".to_string()), None, None]);
    }

    #[test]
    fn a_guide_books_timestamps_are_dropped() {
        let parsed = parse_collection(GUIDE_BOOK).unwrap();

        assert!(parsed.tracks[0].points.iter().all(|p| p.time.is_none()));
    }

    #[test]
    fn a_waypoint_without_a_description_keeps_its_comment() {
        let parsed = parse_collection(GUIDE_SINGLE).unwrap();

        assert_eq!(
            parsed.waypoints[0].description.as_deref(),
            Some("Car park by the station")
        );
        assert_eq!(parsed.waypoints[1].name, None);
    }

    #[test]
    fn waypoints_belong_to_a_track_only_when_the_file_has_exactly_one() {
        assert_eq!(
            parse_collection(GUIDE_SINGLE).unwrap().waypoint_track(),
            Some(0)
        );
        assert_eq!(parse_collection(GUIDE_BOOK).unwrap().waypoint_track(), None);
        assert_eq!(
            parse_collection(WAYPOINTS_ONLY).unwrap().waypoint_track(),
            None
        );
    }

    #[test]
    fn each_track_keeps_a_gpx_of_its_own() {
        let parsed = parse_collection(GUIDE_BOOK).unwrap();

        let own = parse_collection(&parsed.tracks[2].gpx).unwrap();
        assert_eq!(own.tracks.len(), 1);
        assert_eq!(own.tracks[0].name.as_deref(), Some("Stage 3: Albula"));
        // Another track's file does not carry the book's waypoints.
        assert!(own.waypoints.is_empty());
    }

    #[test]
    fn us95_a_single_tours_gpx_keeps_its_waypoints_symbols() {
        let parsed = parse_collection(GUIDE_SINGLE).unwrap();

        let own = parse_collection(&parsed.tracks[0].gpx).unwrap();
        assert_eq!(own.waypoints[0].symbol.as_deref(), Some("Parking Area"));
    }

    #[test]
    fn a_tracks_own_gpx_carries_no_timestamps() {
        // Not when anyone went: a download imported as a trip must not
        // bring them back.
        let parsed = parse_collection(GUIDE_BOOK).unwrap();

        let own = String::from_utf8(parsed.tracks[0].gpx.clone()).unwrap();
        assert!(own.contains("<trkpt"), "{own}");
        assert!(!own.contains("<time>"), "{own}");
    }

    #[test]
    fn a_single_tours_gpx_carries_its_waypoints() {
        let parsed = parse_collection(GUIDE_SINGLE).unwrap();

        let own = parse_collection(&parsed.tracks[0].gpx).unwrap();
        assert_eq!(own.waypoints.len(), 2);
    }

    #[test]
    fn a_file_of_waypoints_alone_is_a_collection() {
        let parsed = parse_collection(WAYPOINTS_ONLY).unwrap();

        assert!(parsed.tracks.is_empty());
        assert_eq!(parsed.waypoints.len(), 2);
    }

    #[test]
    fn a_file_with_neither_a_track_nor_a_waypoint_is_refused() {
        assert!(matches!(
            parse_collection(NO_TRACKS),
            Err(ImportError::Empty)
        ));
    }

    #[test]
    fn a_file_that_is_not_gpx_is_refused() {
        assert!(matches!(
            parse_collection(b"not xml at all"),
            Err(ImportError::Parse(_))
        ));
    }

    #[test]
    fn the_name_and_description_come_from_the_files_metadata_and_categories() {
        let parsed = parse_collection(GUIDE_BOOK).unwrap();

        assert_eq!(parsed.suggested_name("book.gpx"), "Lakes & Passes");
        assert_eq!(
            parsed.suggested_description(),
            "Ten tours around the lakes.\n\nBruckmann, Baukasten"
        );
    }

    #[test]
    fn a_category_written_with_character_references_keeps_its_letters() {
        let gpx = br#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1"
     xmlns:gpxx="http://www.garmin.com/xmlschemas/GpxExtensions/v3">
  <wpt lat="46.5" lon="9.2"><extensions><gpxx:WaypointExtension><gpxx:Categories>
    <gpxx:Category>H&#252;tten &amp; P&#xE4;sse</gpxx:Category>
  </gpxx:Categories></gpxx:WaypointExtension></extensions></wpt>
</gpx>"#;

        let parsed = parse_collection(gpx).unwrap();

        assert_eq!(parsed.categories, ["Hütten & Pässe"]);
    }

    #[test]
    fn without_metadata_the_name_is_the_file_name() {
        let parsed = parse_collection(GUIDE_SINGLE).unwrap();

        assert_eq!(
            parsed.suggested_name("Alpencross-light-Baukasten.gpx"),
            "Alpencross-light-Baukasten"
        );
        assert_eq!(parsed.suggested_description(), "");
    }

    #[test]
    fn a_track_without_a_name_takes_the_collections_and_its_position() {
        assert_eq!(track_name(None, "Lakes", 2), "Lakes 2");
        assert_eq!(track_name(Some("Stage 1"), "Lakes", 1), "Stage 1");
    }
}
