//! The stored track, as the detail screen reads it.
//!
//! The track is a GeoJSON blob in the `track` table (ADR-0003), served
//! verbatim by `GET /api/trips/:id/track.geojson` — geometry for the map and
//! two parallel arrays for the elevation chart, in one fetch (ADR-0025).
//! Unlike the JSON API's other shapes it has no struct on the server side to
//! share (`geojson::build_track_geojson` writes it untyped), so what follows
//! describes only the parts this screen draws; the test parses a blob shaped
//! exactly as the server writes one, which is what keeps the two in step.
//!
//! Turning that into a polyline and a pair of chart series is Rust's job, not
//! the drawing script's ([ADR-0025](../../../docs/adr/0025-js-widget-interop-via-eval.md)),
//! so it lives here where `cargo test` reaches it.

use serde::Deserialize;

/// A track as stored. `properties.timestamps` is the server's own (US-4 reads
/// it back to place photos) and is deliberately not described here.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Track {
    pub geometry: Geometry,
    #[serde(default)]
    pub properties: Properties,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Geometry {
    /// One `[lon, lat, ele]` position per track point, as GeoJSON orders them.
    #[serde(default)]
    pub coordinates: Vec<Vec<f64>>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Properties {
    #[serde(default)]
    pub cumulative_distance_m: Vec<f64>,
    #[serde(default)]
    pub elevation_m: Vec<f64>,
}

/// The track as Leaflet takes it: `[lat, lon]` pairs, in track order. A
/// position carrying fewer than two numbers is left out rather than
/// defaulted — a point at the wrong place on the map is worse than one
/// missing point in a line of thousands.
pub fn polyline(track: &Track) -> Vec<[f64; 2]> {
    track
        .geometry
        .coordinates
        .iter()
        .filter_map(|position| match position[..] {
            [lon, lat, ..] => Some([lat, lon]),
            _ => None,
        })
        .collect()
}

/// The elevation chart's two series: cumulative distance in kilometres (the
/// x axis, the unit the rest of the UI shows distances in) against elevation
/// in metres.
///
/// `None` unless the two arrays are non-empty and the same length: a chart
/// drawn from series that do not line up plots elevations at distances they
/// were never measured at, which is worse than no chart.
///
/// A GPX carrying no elevation at all is *not* that case and still gets a
/// chart: the server writes `0.0` per point (`geojson::build_track_geojson`),
/// so the series stay parallel and the profile is a flat line at zero — the
/// same thing the ascent and descent stats report for such a trip, and what
/// the page this screen replaces drew.
pub fn elevation_series(track: &Track) -> Option<(Vec<f64>, Vec<f64>)> {
    let distance_m = &track.properties.cumulative_distance_m;
    let elevation_m = &track.properties.elevation_m;
    if distance_m.is_empty() || distance_m.len() != elevation_m.len() {
        return None;
    }
    let distance_km = distance_m.iter().map(|m| m / 1000.0).collect();
    Some((distance_km, elevation_m.clone()))
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A blob shaped exactly as the server stores one (`build_track_geojson`,
    /// ADR-0003) — parsing it here is what keeps this reader honest about the
    /// shape it is reading.
    const STORED_TRACK: &str = r#"{
        "type": "Feature",
        "geometry": {
            "type": "LineString",
            "coordinates": [[10.75, 59.91, 12.0], [10.76, 59.92, 30.0]]
        },
        "properties": {
            "cumulative_distance_m": [0.0, 1234.0],
            "elevation_m": [12.0, 30.0],
            "timestamps": ["2026-07-11T09:30:00Z", "2026-07-11T09:35:00Z"]
        }
    }"#;

    fn stored_track() -> Track {
        serde_json::from_str(STORED_TRACK).expect("the stored blob must parse")
    }

    #[test]
    fn a_stored_track_becomes_a_leaflet_polyline() {
        // GeoJSON orders a position `[lon, lat, ele]`; Leaflet takes
        // `[lat, lon]`. The flip is Rust's job (ADR-0025), so it is asserted
        // here rather than trusted to a string of JavaScript.
        assert_eq!(
            polyline(&stored_track()),
            vec![[59.91, 10.75], [59.92, 10.76]]
        );
    }

    #[test]
    fn a_position_without_both_coordinates_is_left_out_of_the_line() {
        let track: Track = serde_json::from_str(
            r#"{"geometry": {"coordinates": [[10.75], [10.76, 59.92]]}, "properties": {}}"#,
        )
        .unwrap();

        assert_eq!(polyline(&track), vec![[59.92, 10.76]]);
    }

    #[test]
    fn the_elevation_series_is_kilometres_against_metres() {
        let (distance_km, elevation_m) =
            elevation_series(&stored_track()).expect("a full track has a chart");

        assert_eq!(distance_km, vec![0.0, 1.234]);
        assert_eq!(elevation_m, vec![12.0, 30.0]);
    }

    #[test]
    fn a_track_without_parallel_series_draws_no_chart() {
        // Half a pair of series is not a chart, it is a misleading one — a
        // blob with no track, or one written by some older build. A GPX
        // without elevation is a different case: the server writes zeroes, so
        // the series line up and the profile is flat rather than absent.
        let empty: Track =
            serde_json::from_str(r#"{"geometry": {"coordinates": []}, "properties": {}}"#).unwrap();
        assert_eq!(elevation_series(&empty), None);

        let lopsided: Track = serde_json::from_str(
            r#"{"geometry": {"coordinates": []},
                "properties": {"cumulative_distance_m": [0.0, 1.0], "elevation_m": [12.0]}}"#,
        )
        .unwrap();
        assert_eq!(elevation_series(&lopsided), None);
    }
}
