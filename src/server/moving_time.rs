//! Moving time (US-77): how long a trip was actually under way, as opposed
//! to `duration_secs`, which runs from the first point to the last and
//! counts every break — and how far it went meanwhile (US-80).
//!
//! Pure, so it is tested directly (ADR-0012); `repo::moving_time` stores
//! what it computes.

use crate::config;
use crate::models::ActivityType;
use crate::server::geojson;
use crate::server::gpx::TimedPoint;

/// How long, and how far, a trip moved: the time and the distance between
/// consecutive timed points whose speed reaches the activity's threshold.
/// Counted over the same pairs, so their ratio is the average speed in
/// motion (US-80).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moving {
    pub secs: i64,
    pub distance_m: f64,
}

/// The time and distance between consecutive points (sorted by time, as
/// [`gpx::timed_points`](crate::server::gpx::timed_points) gives them) whose
/// speed reaches `min_speed_kmh`. `None` with fewer than two timed points —
/// a track with no times has no moving time, which is not the same as none.
pub fn moving(points: &[TimedPoint], min_speed_kmh: f64) -> Option<Moving> {
    use geo::HaversineDistance;

    if points.len() < 2 {
        return None;
    }
    let min_speed_ms = min_speed_kmh / 3.6;
    let mut moving = Moving {
        secs: 0,
        distance_m: 0.0,
    };
    for pair in points.windows(2) {
        let secs = (pair[1].time - pair[0].time).whole_seconds();
        // Two points at one instant cover no time, moving or not.
        if secs <= 0 {
            continue;
        }
        let metres = geo::Point::new(pair[0].lon, pair[0].lat)
            .haversine_distance(&geo::Point::new(pair[1].lon, pair[1].lat));
        if metres / secs as f64 >= min_speed_ms {
            moving.secs += secs;
            moving.distance_m += metres;
        }
    }
    Some(moving)
}

/// How a stored track blob moved, under the threshold of the trip's
/// activity.
pub fn of_track(geojson: &str, activity: ActivityType) -> Option<Moving> {
    moving(
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
    fn moving_secs(points: &[TimedPoint], min_speed_kmh: f64) -> Option<i64> {
        moving(points, min_speed_kmh).map(|moving| moving.secs)
    }

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

        let secs = |activity| of_track(&blob, activity).map(|moving| moving.secs);
        assert_eq!(secs(ActivityType::Hiking), Some(3600));
        assert_eq!(secs(ActivityType::Kayaking), Some(1800));
        assert_eq!(secs(ActivityType::Cycling), Some(0));
    }

    /// One step of [`point`] in metres, as the haversine measures it.
    const STEP_M: f64 = 111.19;

    fn approx(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.5,
            "{actual} is not ~{expected}"
        );
    }

    #[test]
    fn us80_only_the_distance_covered_while_moving_counts() {
        // The second step takes ten minutes, ~0.67 km/h: under a 1 km/h
        // threshold neither its time nor its distance counts.
        let points = [point(0, 0.0), point(1, 1.0), point(11, 2.0)];

        let moving = moving(&points, 1.0).unwrap();

        assert_eq!(moving.secs, 60);
        approx(moving.distance_m, STEP_M);
        approx(
            super::moving(&points, 0.5).unwrap().distance_m,
            2.0 * STEP_M,
        );
    }

    #[test]
    fn us80_a_break_lowers_neither_figure() {
        let without = [point(0, 0.0), point(1, 1.0), point(2, 2.0)];
        let with = [point(0, 0.0), point(1, 1.0), point(31, 1.0), point(32, 2.0)];

        assert_eq!(moving(&without, 1.0), moving(&with, 1.0));
        approx(moving(&with, 1.0).unwrap().distance_m, 2.0 * STEP_M);
    }

    #[test]
    fn us80_a_track_without_times_has_no_moving_distance() {
        assert_eq!(moving(&[point(0, 0.0)], 1.0), None);
    }
}
