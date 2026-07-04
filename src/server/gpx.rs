use time::OffsetDateTime;

use crate::server::error::ImportError;

// ── Domain types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TrackPoint {
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    pub time: Option<OffsetDateTime>,
}

#[derive(Debug)]
pub struct ParsedTrack {
    pub name: Option<String>,
    pub points: Vec<TrackPoint>,
}

/// Derived statistics computed from the raw track points.
/// All distance/elevation values are in metres; times are UTC offset datetimes.
#[derive(Debug)]
pub struct TrackStats {
    pub distance_m: f64,
    pub ascent_m: f64,
    pub descent_m: f64,
    pub duration_secs: Option<i64>,
    pub start_time: Option<OffsetDateTime>,
    pub end_time: Option<OffsetDateTime>,
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_lat: f64,
    pub max_lon: f64,
}

// ── GPX parsing ──────────────────────────────────────────────────────────────

/// Parse raw GPX bytes into a `ParsedTrack`.
///
/// Returns `ImportError::NoTrack` for a valid GPX with no `<trk>` elements,
/// `ImportError::NoPoints` if the first track has empty segments,
/// and `ImportError::Parse` for malformed XML.
pub fn parse_gpx(data: &[u8]) -> Result<ParsedTrack, ImportError> {
    let cursor = std::io::Cursor::new(data);
    let gpx_data: gpx::Gpx = gpx::read(cursor).map_err(|e| ImportError::Parse(e.to_string()))?;

    let track = gpx_data
        .tracks
        .into_iter()
        .next()
        .ok_or(ImportError::NoTrack)?;

    let name = track.name.clone();
    let points: Vec<TrackPoint> = track
        .segments
        .into_iter()
        .flat_map(|seg| seg.points)
        .map(|wp| {
            // gpx::Time wraps time::OffsetDateTime — use the public From conversion.
            let time: Option<OffsetDateTime> = wp.time.map(|t| t.into());
            TrackPoint {
                lat: wp.point().y(),
                lon: wp.point().x(),
                ele: wp.elevation,
                time,
            }
        })
        .collect();

    if points.is_empty() {
        return Err(ImportError::NoPoints);
    }

    Ok(ParsedTrack { name, points })
}

// ── Statistics ───────────────────────────────────────────────────────────────

/// Derive stats from the track points using haversine distances (US-8).
///
/// Pure function — no I/O, no side effects — unit-testable directly (ADR-0012).
pub fn compute_stats(points: &[TrackPoint]) -> TrackStats {
    use geo::HaversineDistance;

    let mut distance_m = 0.0_f64;
    let mut ascent_m = 0.0_f64;
    let mut descent_m = 0.0_f64;

    for w in points.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let ga = geo::Point::new(a.lon, a.lat);
        let gb = geo::Point::new(b.lon, b.lat);
        distance_m += ga.haversine_distance(&gb);

        if let (Some(ea), Some(eb)) = (a.ele, b.ele) {
            let diff = eb - ea;
            if diff > 0.0 {
                ascent_m += diff;
            } else {
                descent_m += -diff;
            }
        }
    }

    let min_lat = points.iter().map(|p| p.lat).fold(f64::INFINITY, f64::min);
    let max_lat = points
        .iter()
        .map(|p| p.lat)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_lon = points.iter().map(|p| p.lon).fold(f64::INFINITY, f64::min);
    let max_lon = points
        .iter()
        .map(|p| p.lon)
        .fold(f64::NEG_INFINITY, f64::max);

    let start_time = points.first().and_then(|p| p.time);
    let end_time = points.last().and_then(|p| p.time);
    let duration_secs = match (start_time, end_time) {
        (Some(s), Some(e)) => Some((e - s).whole_seconds()),
        _ => None,
    };

    TrackStats {
        distance_m,
        ascent_m,
        descent_m,
        duration_secs,
        start_time,
        end_time,
        min_lat,
        min_lon,
        max_lat,
        max_lon,
    }
}

// ── Timestamp interpolation (US-4, ADR-0009) ─────────────────────────────────

/// A track point reduced to just what interpolation needs.
#[derive(Debug, Clone, Copy)]
pub struct TimedPoint {
    pub time: OffsetDateTime,
    pub lat: f64,
    pub lon: f64,
}

/// Points with a timestamp, sorted ascending by time (a stable sort — ties
/// keep their original order). GPX doesn't guarantee `<time>` is monotonic
/// across `<trkpt>`s (paused/resumed recordings, clock jumps); this is the
/// only place that invariant is established, so callers build this once per
/// trip, not once per photo.
pub fn timed_points(points: &[TrackPoint]) -> Vec<TimedPoint> {
    let mut timed: Vec<TimedPoint> = points
        .iter()
        .filter_map(|p| {
            p.time.map(|time| TimedPoint {
                time,
                lat: p.lat,
                lon: p.lon,
            })
        })
        .collect();
    timed.sort_by_key(|p| p.time);
    timed
}

/// Estimate a position at `at` by linearly interpolating between the
/// bracketing points in `timed_points` (must already be sorted ascending by
/// time — see `timed_points`). `None` if fewer than two points have a
/// timestamp, or `at` falls strictly outside `[first.time, last.time]`;
/// the endpoints themselves are inclusive (US-4: "falls within the track
/// range"). A run of points sharing an identical timestamp is handled by a
/// zero-length-span guard rather than dividing by zero: when `at` matches the
/// run exactly, the bracket search lands on the last point of the run.
pub fn interpolate_position(timed_points: &[TimedPoint], at: OffsetDateTime) -> Option<(f64, f64)> {
    let first = timed_points.first()?;
    let last = timed_points.last()?;
    if timed_points.len() < 2 || at < first.time || at > last.time {
        return None;
    }

    let idx = timed_points.partition_point(|p| p.time <= at);
    if idx == 0 {
        return Some((first.lat, first.lon));
    }
    if idx == timed_points.len() {
        return Some((last.lat, last.lon));
    }

    let a = &timed_points[idx - 1];
    let b = &timed_points[idx];
    let span = (b.time - a.time).as_seconds_f64();
    let elapsed = (at - a.time).as_seconds_f64();
    let frac = if span > 0.0 { elapsed / span } else { 0.0 };

    Some((
        a.lat + (b.lat - a.lat) * frac,
        a.lon + (b.lon - a.lon) * frac,
    ))
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use time::format_description::well_known::Rfc3339;
    use time::macros::datetime;

    const SAMPLE_GPX: &[u8] = include_bytes!("../../tests/fixtures/sample.gpx");
    const NO_TRACKS_GPX: &[u8] = include_bytes!("../../tests/fixtures/no_tracks.gpx");

    // ── US-1: parse a valid GPX file ─────────────────────────────────────────

    #[test]
    fn us1_parse_valid_gpx_returns_three_points() {
        let track = parse_gpx(SAMPLE_GPX).expect("parse should succeed");
        assert_eq!(track.points.len(), 3, "fixture has 3 track points");
    }

    #[test]
    fn us1_parse_valid_gpx_preserves_track_name() {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        assert_eq!(track.name.as_deref(), Some("Oslo Hills Walk"));
    }

    #[test]
    fn us1_parse_valid_gpx_captures_coordinates() {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let first = &track.points[0];
        assert!((first.lat - 59.9139).abs() < 1e-4, "lat matches fixture");
        assert!((first.lon - 10.7522).abs() < 1e-4, "lon matches fixture");
    }

    #[test]
    fn us1_parse_valid_gpx_captures_elevation() {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        assert_eq!(track.points[0].ele, Some(10.0));
        assert_eq!(track.points[1].ele, Some(50.0));
        assert_eq!(track.points[2].ele, Some(30.0));
    }

    #[test]
    fn us1_parse_valid_gpx_captures_timestamps() {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let t = track.points[0].time.expect("first point has a timestamp");
        let formatted = t.format(&Rfc3339).unwrap();
        assert!(
            formatted.starts_with("2024-06-01T08:00:00"),
            "timestamp should be 2024-06-01T08:00:00, got {formatted}"
        );
    }

    #[test]
    fn us1_parse_gpx_with_no_tracks_returns_no_track_error() {
        let err = parse_gpx(NO_TRACKS_GPX).unwrap_err();
        assert!(
            matches!(err, ImportError::NoTrack),
            "expected NoTrack, got {err:?}"
        );
    }

    #[test]
    fn us1_parse_invalid_bytes_returns_parse_error() {
        let err = parse_gpx(b"not xml at all").unwrap_err();
        assert!(matches!(err, ImportError::Parse(_)));
    }

    // ── US-8: stats are derived automatically ────────────────────────────────

    #[test]
    fn us8_compute_stats_distance_is_positive_and_realistic() {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let stats = compute_stats(&track.points);
        assert!(
            stats.distance_m > 1_000.0 && stats.distance_m < 2_500.0,
            "distance {:.1} m not in expected range",
            stats.distance_m
        );
    }

    #[test]
    fn us8_compute_stats_ascent_is_40m() {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let stats = compute_stats(&track.points);
        assert!(
            (stats.ascent_m - 40.0).abs() < 0.1,
            "ascent {}",
            stats.ascent_m
        );
    }

    #[test]
    fn us8_compute_stats_descent_is_20m() {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let stats = compute_stats(&track.points);
        assert!(
            (stats.descent_m - 20.0).abs() < 0.1,
            "descent {}",
            stats.descent_m
        );
    }

    #[test]
    fn us8_compute_stats_duration_is_3600s() {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let stats = compute_stats(&track.points);
        assert_eq!(stats.duration_secs, Some(3600));
    }

    #[test]
    fn us8_compute_stats_bbox_covers_all_points() {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let stats = compute_stats(&track.points);
        assert!((stats.min_lat - 59.9139).abs() < 1e-4);
        assert!((stats.max_lat - 59.9250).abs() < 1e-4);
        assert!((stats.min_lon - 10.7522).abs() < 1e-4);
        assert!((stats.max_lon - 10.7650).abs() < 1e-4);
    }

    #[test]
    fn us8_compute_stats_start_end_times_are_correct() {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let stats = compute_stats(&track.points);
        let start = stats.start_time.unwrap().format(&Rfc3339).unwrap();
        let end = stats.end_time.unwrap().format(&Rfc3339).unwrap();
        assert!(start.starts_with("2024-06-01T08:00:00"), "start={start}");
        assert!(end.starts_with("2024-06-01T09:00:00"), "end={end}");
    }

    // ── US-4: timestamp interpolation ────────────────────────────────────────

    fn point(lat: f64, lon: f64, time: Option<OffsetDateTime>) -> TrackPoint {
        TrackPoint {
            lat,
            lon,
            ele: None,
            time,
        }
    }

    #[test]
    fn timed_points_filters_out_points_without_a_timestamp() {
        let points = vec![
            point(1.0, 1.0, Some(datetime!(2024-01-01 08:00 UTC))),
            point(2.0, 2.0, None),
            point(3.0, 3.0, Some(datetime!(2024-01-01 09:00 UTC))),
        ];
        assert_eq!(timed_points(&points).len(), 2);
    }

    #[test]
    fn timed_points_sorts_out_of_order_input() {
        let points = vec![
            point(2.0, 2.0, Some(datetime!(2024-01-01 09:00 UTC))),
            point(1.0, 1.0, Some(datetime!(2024-01-01 08:00 UTC))),
        ];
        let timed = timed_points(&points);
        assert_eq!(timed[0].lat, 1.0);
        assert_eq!(timed[1].lat, 2.0);
    }

    #[test]
    fn interpolate_position_returns_none_with_fewer_than_two_timed_points() {
        let timed = timed_points(&[point(1.0, 1.0, Some(datetime!(2024-01-01 08:00 UTC)))]);
        assert!(interpolate_position(&timed, datetime!(2024-01-01 08:00 UTC)).is_none());
    }

    #[test]
    fn interpolate_position_returns_none_before_the_track_range() {
        let timed = timed_points(&[
            point(1.0, 1.0, Some(datetime!(2024-01-01 08:00 UTC))),
            point(2.0, 2.0, Some(datetime!(2024-01-01 09:00 UTC))),
        ]);
        assert!(interpolate_position(&timed, datetime!(2024-01-01 07:00 UTC)).is_none());
    }

    #[test]
    fn interpolate_position_returns_none_after_the_track_range() {
        let timed = timed_points(&[
            point(1.0, 1.0, Some(datetime!(2024-01-01 08:00 UTC))),
            point(2.0, 2.0, Some(datetime!(2024-01-01 09:00 UTC))),
        ]);
        assert!(interpolate_position(&timed, datetime!(2024-01-01 10:00 UTC)).is_none());
    }

    #[test]
    fn interpolate_position_matches_the_first_point_exactly() {
        let timed = timed_points(&[
            point(1.0, 1.0, Some(datetime!(2024-01-01 08:00 UTC))),
            point(2.0, 2.0, Some(datetime!(2024-01-01 09:00 UTC))),
        ]);
        let (lat, lon) = interpolate_position(&timed, datetime!(2024-01-01 08:00 UTC)).unwrap();
        assert_eq!((lat, lon), (1.0, 1.0));
    }

    #[test]
    fn interpolate_position_matches_the_last_point_exactly() {
        let timed = timed_points(&[
            point(1.0, 1.0, Some(datetime!(2024-01-01 08:00 UTC))),
            point(2.0, 2.0, Some(datetime!(2024-01-01 09:00 UTC))),
        ]);
        let (lat, lon) = interpolate_position(&timed, datetime!(2024-01-01 09:00 UTC)).unwrap();
        assert_eq!((lat, lon), (2.0, 2.0));
    }

    #[test]
    fn interpolate_position_linearly_interpolates_the_midpoint() {
        let timed = timed_points(&[
            point(0.0, 0.0, Some(datetime!(2024-01-01 08:00 UTC))),
            point(10.0, 20.0, Some(datetime!(2024-01-01 10:00 UTC))),
        ]);
        let (lat, lon) = interpolate_position(&timed, datetime!(2024-01-01 09:00 UTC)).unwrap();
        assert!((lat - 5.0).abs() < 1e-9);
        assert!((lon - 10.0).abs() < 1e-9);
    }

    #[test]
    fn interpolate_position_handles_duplicate_timestamps_without_dividing_by_zero() {
        // When `at` matches a run of duplicate timestamps, the bracket search
        // lands on the last of the run (deterministic, not a panic/NaN).
        let timed = timed_points(&[
            point(1.0, 1.0, Some(datetime!(2024-01-01 08:00 UTC))),
            point(2.0, 2.0, Some(datetime!(2024-01-01 08:00 UTC))),
            point(3.0, 3.0, Some(datetime!(2024-01-01 09:00 UTC))),
        ]);
        let (lat, lon) = interpolate_position(&timed, datetime!(2024-01-01 08:00 UTC)).unwrap();
        assert_eq!((lat, lon), (2.0, 2.0));
    }
}
