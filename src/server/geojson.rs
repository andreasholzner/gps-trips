use crate::server::gpx::{TimedPoint, TrackPoint};
use crate::server::timezone;

/// Build the GeoJSON blob stored in the `track` table (ADR-0003).
///
/// Format: a GeoJSON Feature with:
/// - geometry: LineString with [lon, lat, ele] coordinates
/// - properties: parallel arrays (cumulative distance, elevation, timestamps)
///   for a future elevation chart (ADR-0006)
///
/// Pure function — unit-testable directly (ADR-0012).
pub fn build_track_geojson(points: &[TrackPoint]) -> String {
    use geo::HaversineDistance;

    let coordinates: Vec<serde_json::Value> = points
        .iter()
        .map(|p| serde_json::json!([p.lon, p.lat, p.ele.unwrap_or(0.0)]))
        .collect();

    // Cumulative distances from the first point — used as the x-axis of the elevation chart.
    let mut cumulative_m = vec![0.0_f64];
    let mut running = 0.0_f64;
    for w in points.windows(2) {
        let ga = geo::Point::new(w[0].lon, w[0].lat);
        let gb = geo::Point::new(w[1].lon, w[1].lat);
        running += ga.haversine_distance(&gb);
        cumulative_m.push(running);
    }

    let elevations: Vec<f64> = points.iter().map(|p| p.ele.unwrap_or(0.0)).collect();
    let timestamps: Vec<String> = points
        .iter()
        .map(|p| {
            p.time
                .and_then(|t| {
                    t.format(&time::format_description::well_known::Rfc3339)
                        .ok()
                })
                .unwrap_or_default()
        })
        .collect();

    serde_json::json!({
        "type": "Feature",
        "geometry": {
            "type": "LineString",
            "coordinates": coordinates
        },
        "properties": {
            "cumulative_distance_m": cumulative_m,
            "elevation_m": elevations,
            "timestamps": timestamps
        }
    })
    .to_string()
}

/// Parse a stored track GeoJSON blob back into timed points (US-4): the
/// inverse of `build_track_geojson`'s `coordinates`/`properties.timestamps`
/// arrays. `handle_add_photos` uses this instead of re-parsing the original
/// GPX XML (which it doesn't have in memory the way `handle_import` does) —
/// the trip's track geometry is already parsed and stored, so reading it
/// back is cheaper than a full XML re-parse. Malformed JSON, or a point whose
/// timestamp is empty/unparseable (mirrors `build_track_geojson` writing
/// `""` for a GPX point with no `<time>`), is skipped rather than failing
/// the whole parse — same best-effort spirit as `gpx::timed_points`.
pub fn parse_timed_points(geojson: &str) -> Vec<TimedPoint> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(geojson) else {
        return Vec::new();
    };
    let mut timed: Vec<TimedPoint> = indexed_timed_points(&value)
        .into_iter()
        .map(|(_, point)| point)
        .collect();
    timed.sort_by_key(|p| p.time);
    timed
}

/// A stored track blob as it is served (US-62): verbatim (ADR-0003), plus
/// `properties.utc_offsets` — the offset in force from each index at which it
/// changes, `[[0, 7200], [1841, 10800]]`, in seconds east of UTC, or `null`
/// from an index whose zone this build's tzdata cannot resolve.
///
/// Indices are the blob's own, the ones the elevation chart reports; a point
/// with no timestamp has no instant to take an offset at and carries the one
/// before it. Computed per request, never stored: the boundaries and tzdata it
/// reads are versioned apart from the archive, so the best answer is today's.
/// A blob this cannot read is served as it is — the geometry still draws.
pub fn with_utc_offsets(geojson: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(geojson) else {
        return geojson.to_string();
    };
    let changes = timezone::offset_changes(indexed_timed_points(&value));
    let Some(properties) = value["properties"].as_object_mut() else {
        return geojson.to_string();
    };
    let offsets: Vec<serde_json::Value> = changes
        .into_iter()
        .map(|(index, offset)| serde_json::json!([index, offset.map(|o| o.whole_seconds())]))
        .collect();
    properties.insert("utc_offsets".to_string(), offsets.into());
    value.to_string()
}

/// Each point that has a timestamp, under its index in the blob, in blob
/// order. Shared by [`parse_timed_points`], which then sorts them by time.
fn indexed_timed_points(value: &serde_json::Value) -> Vec<(usize, TimedPoint)> {
    let (Some(coordinates), Some(timestamps)) = (
        value["geometry"]["coordinates"].as_array(),
        value["properties"]["timestamps"].as_array(),
    ) else {
        return Vec::new();
    };
    coordinates
        .iter()
        .zip(timestamps)
        .enumerate()
        .filter_map(|(index, (coord, ts))| {
            let lon = coord.get(0)?.as_f64()?;
            let lat = coord.get(1)?.as_f64()?;
            let time = time::OffsetDateTime::parse(
                ts.as_str()?,
                &time::format_description::well_known::Rfc3339,
            )
            .ok()?;
            Some((index, TimedPoint { time, lat, lon }))
        })
        .collect()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::gpx::parse_gpx;

    const SAMPLE_GPX: &[u8] = include_bytes!("../../tests/fixtures/sample.gpx");

    fn sample_geojson() -> serde_json::Value {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let raw = build_track_geojson(&track.points);
        serde_json::from_str(&raw).expect("valid JSON")
    }

    // ── US-1: GeoJSON is produced on import ──────────────────────────────────

    #[test]
    fn us1_geojson_is_a_feature() {
        let j = sample_geojson();
        assert_eq!(j["type"], "Feature");
    }

    #[test]
    fn us1_geojson_geometry_is_linestring() {
        let j = sample_geojson();
        assert_eq!(j["geometry"]["type"], "LineString");
    }

    #[test]
    fn us1_geojson_has_three_coordinate_pairs() {
        let j = sample_geojson();
        assert_eq!(j["geometry"]["coordinates"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn us1_geojson_coordinates_are_lon_lat_ele() {
        let j = sample_geojson();
        let first = &j["geometry"]["coordinates"][0];
        // [lon, lat, ele]
        assert!((first[0].as_f64().unwrap() - 10.7522).abs() < 1e-4, "lon");
        assert!((first[1].as_f64().unwrap() - 59.9139).abs() < 1e-4, "lat");
        assert!((first[2].as_f64().unwrap() - 10.0).abs() < 1e-4, "ele");
    }

    #[test]
    fn us1_geojson_properties_contain_elevation_array() {
        let j = sample_geojson();
        let elev = j["properties"]["elevation_m"].as_array().unwrap();
        assert_eq!(elev.len(), 3);
        assert!((elev[0].as_f64().unwrap() - 10.0).abs() < 0.1);
        assert!((elev[1].as_f64().unwrap() - 50.0).abs() < 0.1);
        assert!((elev[2].as_f64().unwrap() - 30.0).abs() < 0.1);
    }

    #[test]
    fn us1_geojson_properties_contain_cumulative_distance() {
        let j = sample_geojson();
        let dist = j["properties"]["cumulative_distance_m"].as_array().unwrap();
        assert_eq!(dist.len(), 3, "one entry per point");
        assert_eq!(dist[0].as_f64().unwrap(), 0.0, "first entry is always 0");
        assert!(
            dist[1].as_f64().unwrap() > dist[0].as_f64().unwrap(),
            "distances increase"
        );
        assert!(dist[2].as_f64().unwrap() > dist[1].as_f64().unwrap());
    }

    #[test]
    fn us1_geojson_properties_contain_timestamps() {
        let j = sample_geojson();
        let ts = j["properties"]["timestamps"].as_array().unwrap();
        assert_eq!(ts.len(), 3);
        assert!(ts[0].as_str().unwrap().contains("2024-06-01"));
    }

    // ── US-4: parse_timed_points (the inverse used by handle_add_photos) ────

    #[test]
    fn parse_timed_points_round_trips_a_built_geojson_blob() {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let geojson = build_track_geojson(&track.points);

        let timed = parse_timed_points(&geojson);
        assert_eq!(timed.len(), 3);
        assert!((timed[0].lat - 59.9139).abs() < 1e-4);
        assert!((timed[0].lon - 10.7522).abs() < 1e-4);
    }

    #[test]
    fn parse_timed_points_returns_empty_for_malformed_json() {
        assert!(parse_timed_points("not json").is_empty());
    }

    #[test]
    fn parse_timed_points_skips_points_with_an_empty_timestamp() {
        let geojson = serde_json::json!({
            "geometry": { "coordinates": [[10.0, 59.0, 0.0], [11.0, 60.0, 0.0]] },
            "properties": { "timestamps": ["2024-06-01T08:00:00Z", ""] }
        })
        .to_string();

        let timed = parse_timed_points(&geojson);
        assert_eq!(timed.len(), 1);
        assert!((timed[0].lon - 10.0).abs() < 1e-9);
    }

    #[test]
    fn parse_timed_points_sorts_out_of_order_timestamps() {
        let geojson = serde_json::json!({
            "geometry": { "coordinates": [[11.0, 60.0, 0.0], [10.0, 59.0, 0.0]] },
            "properties": { "timestamps": ["2024-06-01T09:00:00Z", "2024-06-01T08:00:00Z"] }
        })
        .to_string();

        let timed = parse_timed_points(&geojson);
        assert!(
            (timed[0].lon - 10.0).abs() < 1e-9,
            "earlier timestamp first"
        );
        assert!((timed[1].lon - 11.0).abs() < 1e-9);
    }

    // ── US-62: the track as it is served ────────────────────────────────────

    /// Karasjok (Europe/Oslo) and Inari (Europe/Helsinki), either side of
    /// the border — real coordinates, as `timezone`'s own tests use.
    fn border_track(timestamps: [&str; 3]) -> String {
        serde_json::json!({
            "type": "Feature",
            "geometry": { "type": "LineString", "coordinates": [
                [25.514, 69.472, 0.0], [25.6, 69.4, 0.0], [27.029, 68.906, 0.0]
            ] },
            "properties": { "elevation_m": [0.0, 0.0, 0.0], "timestamps": timestamps }
        })
        .to_string()
    }

    #[test]
    fn the_served_track_says_where_the_offset_changes_by_the_charts_index() {
        // The middle point has no time, so among the timed points Inari is
        // the second; in the blob — which is what the chart indexes — it is
        // the third, and that is the index the change must be reported at.
        let blob = border_track(["2024-06-01T08:00:00Z", "", "2024-06-01T10:00:00Z"]);

        let served: serde_json::Value = serde_json::from_str(&with_utc_offsets(&blob)).unwrap();

        assert_eq!(
            served["properties"]["utc_offsets"],
            serde_json::json!([[0, 7200], [2, 10800]])
        );
    }

    #[test]
    fn the_served_track_is_the_stored_one_otherwise() {
        // ADR-0003: the blob is stored verbatim, and nothing it carries is
        // changed on its way out.
        let blob = border_track(["2024-06-01T08:00:00Z", "", "2024-06-01T10:00:00Z"]);

        let mut served: serde_json::Value = serde_json::from_str(&with_utc_offsets(&blob)).unwrap();
        served["properties"]
            .as_object_mut()
            .unwrap()
            .remove("utc_offsets");

        assert_eq!(
            served,
            serde_json::from_str::<serde_json::Value>(&blob).unwrap()
        );
    }

    #[test]
    fn a_track_with_no_times_is_served_with_no_offsets() {
        let blob = border_track(["", "", ""]);

        let served: serde_json::Value = serde_json::from_str(&with_utc_offsets(&blob)).unwrap();

        assert_eq!(served["properties"]["utc_offsets"], serde_json::json!([]));
    }

    #[test]
    fn a_blob_that_cannot_be_read_is_served_as_it_is() {
        assert_eq!(with_utc_offsets("not json"), "not json");
    }
}
