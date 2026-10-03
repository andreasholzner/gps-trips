//! Moving time (US-77): how long a trip was actually under way, as opposed
//! to `duration_secs`, which runs from the first point to the last and
//! counts every break.
//!
//! Pure, so it is tested directly (ADR-0012); `repo::moving_time` stores
//! what it computes.

use crate::config;
use crate::models::ActivityType;
use crate::server::geojson;
use crate::server::gpx::TimedPoint;

/// The seconds between consecutive points (sorted by time, as
/// [`gpx::timed_points`](crate::server::gpx::timed_points) gives them) whose
/// speed reaches `min_speed_kmh`. `None` with fewer than two timed points —
/// a track with no times has no moving time, which is not the same as none.
pub fn moving_secs(points: &[TimedPoint], min_speed_kmh: f64) -> Option<i64> {
    use geo::HaversineDistance;

    if points.len() < 2 {
        return None;
    }
    let min_speed_ms = min_speed_kmh / 3.6;
    let moving = points
        .windows(2)
        .filter_map(|pair| {
            let secs = (pair[1].time - pair[0].time).whole_seconds();
            // Two points at one instant cover no time, moving or not.
            if secs <= 0 {
                return None;
            }
            let metres = geo::Point::new(pair[0].lon, pair[0].lat)
                .haversine_distance(&geo::Point::new(pair[1].lon, pair[1].lat));
            (metres / secs as f64 >= min_speed_ms).then_some(secs)
        })
        .sum();
    Some(moving)
}

/// The moving time of a stored track blob, under the threshold of the
/// trip's activity.
pub fn of_track(geojson: &str, activity: ActivityType) -> Option<i64> {
    moving_secs(
        &geojson::parse_timed_points(geojson),
        config::moving_time::min_speed_kmh(activity),
    )
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    /// About 111 m north of the previous point per step: a degree of
    /// latitude is ~111 km.
    fn point(minutes: i64, north_steps: f64) -> TimedPoint {
        TimedPoint {
            time: datetime!(2024-06-01 08:00 UTC) + time::Duration::minutes(minutes),
            lat: 60.0 + north_steps * 0.001,
            lon: 10.0,
        }
    }

    #[test]
    fn us77_time_spent_at_or_above_the_threshold_counts_as_moving() {
        // 111 m in a minute is ~6.7 km/h; the same in ten minutes ~0.67 km/h.
        let points = [point(0, 0.0), point(1, 1.0), point(11, 2.0)];

        assert_eq!(moving_secs(&points, 1.0), Some(60));
        assert_eq!(moving_secs(&points, 0.5), Some(660));
        assert_eq!(moving_secs(&points, 7.0), Some(0));
    }

    #[test]
    fn us77_a_break_in_one_place_is_not_moving() {
        let points = [point(0, 0.0), point(1, 1.0), point(31, 1.0), point(32, 2.0)];

        assert_eq!(moving_secs(&points, 1.0), Some(120));
    }

    #[test]
    fn us77_a_track_with_fewer_than_two_timed_points_has_no_moving_time() {
        assert_eq!(moving_secs(&[], 1.0), None);
        assert_eq!(moving_secs(&[point(0, 0.0)], 1.0), None);
    }

    #[test]
    fn us77_points_at_one_instant_add_nothing() {
        let points = [point(0, 0.0), point(0, 1.0), point(1, 2.0)];

        assert_eq!(moving_secs(&points, 1.0), Some(60));
    }

    #[test]
    fn us77_a_stored_track_is_measured_under_its_activitys_threshold() {
        // sample.gpx: two half-hour legs at ~1.6 and ~1.2 km/h.
        let track =
            crate::server::gpx::parse_gpx(include_bytes!("../../tests/fixtures/sample.gpx"))
                .unwrap();
        let blob = geojson::build_track_geojson(&track.points);

        assert_eq!(of_track(&blob, ActivityType::Hiking), Some(3600));
        assert_eq!(of_track(&blob, ActivityType::Kayaking), Some(1800));
        assert_eq!(of_track(&blob, ActivityType::Cycling), Some(0));
    }
}
