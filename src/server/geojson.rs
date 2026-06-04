use crate::server::gpx::TrackPoint;

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
}
