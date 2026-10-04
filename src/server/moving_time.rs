//! Moving time (US-77): how long a trip was actually under way, as opposed
//! to `duration_secs`, which runs from the first point to the last and
//! counts every break — and how far it went meanwhile (US-80). Worked out
//! in track order on the trip's [`Profile`], the same way for the whole
//! trip as for each of its climbs (US-81).
//!
//! Pure, so it is tested directly (ADR-0012); `repo::moving_time` stores
//! what it computes.

use crate::config::{
    self,
    moving_time::{MIN_VERTICAL_MH, VERTICAL_WINDOW_S},
};
use crate::models::ActivityType;
use crate::server::profile::Profile;

/// How long, and how far, a trip moved: the time and the distance between
/// consecutive timed points that count as moving. Counted over the same
/// pairs, so their ratio is the average speed in motion (US-80).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moving {
    pub secs: i64,
    pub distance_m: f64,
}

/// For each pair of consecutive points, in track order: its seconds and
/// whether it counts as moving, or `None` where one of the two has no time
/// or time does not move forward between them.
///
/// A pair moves if its speed over the ground reaches `min_speed_kmh`, or if
/// the smoothed elevation changes at [`MIN_VERTICAL_MH`] or more over the
/// [`VERTICAL_WINDOW_S`] around it (US-81) — a slow, steep climb is moving
/// too.
pub fn steps(profile: &Profile, min_speed_kmh: f64) -> Vec<Option<(i64, bool)>> {
    let smooth = profile.smoothed();
    let seconds = &profile.seconds;
    let min_speed_ms = min_speed_kmh / 3.6;
    (0..seconds.len().saturating_sub(1))
        .map(|i| {
            let (from, to) = (seconds[i]?, seconds[i + 1]?);
            let secs = to - from;
            if secs <= 0 {
                return None;
            }
            let metres = profile.distance_m[i + 1] - profile.distance_m[i];
            let fast = metres / secs as f64 >= min_speed_ms;
            Some((secs, fast || climbing(seconds, &smooth, i)))
        })
        .collect()
}

/// Whether the smoothed elevation changes at [`MIN_VERTICAL_MH`] or more
/// over [`VERTICAL_WINDOW_S`] around the pair starting at `i`, within the
/// run of timed points it lies in.
fn climbing(seconds: &[Option<i64>], smooth: &[f64], i: usize) -> bool {
    let half = VERTICAL_WINDOW_S / 2;
    let (Some(start), Some(end)) = (seconds[i], seconds[i + 1]) else {
        return false;
    };
    let mut before = i;
    while before > 0 {
        match seconds[before - 1] {
            Some(t) if t <= seconds[before].unwrap_or(t) && start - t <= half => before -= 1,
            _ => break,
        }
    }
    let mut after = i + 1;
    while after + 1 < seconds.len() {
        match seconds[after + 1] {
            Some(t) if t >= seconds[after].unwrap_or(t) && t - end <= half => after += 1,
            _ => break,
        }
    }
    let (Some(t0), Some(t1)) = (seconds[before], seconds[after]) else {
        return false;
    };
    let span = (t1 - t0) as f64;
    span > 0.0 && (smooth[after] - smooth[before]).abs() / span * 3600.0 >= MIN_VERTICAL_MH
}

/// What `steps` from `from` to `to` add up to; `None` when none of them is
/// timed — a stretch without times has no moving time, which is not the
/// same as none.
pub fn sum(
    profile: &Profile,
    steps: &[Option<(i64, bool)>],
    from: usize,
    to: usize,
) -> Option<Moving> {
    let mut timed = false;
    let mut moving = Moving {
        secs: 0,
        distance_m: 0.0,
    };
    for (i, step) in steps.iter().enumerate().take(to).skip(from) {
        let Some((secs, is_moving)) = *step else {
            continue;
        };
        timed = true;
        if is_moving {
            moving.secs += secs;
            moving.distance_m += profile.distance_m[i + 1] - profile.distance_m[i];
        }
    }
    timed.then_some(moving)
}

/// How the whole of `profile` moved under `min_speed_kmh`.
pub fn moving(profile: &Profile, min_speed_kmh: f64) -> Option<Moving> {
    let steps = steps(profile, min_speed_kmh);
    sum(profile, &steps, 0, steps.len())
}

/// How a stored track blob moved, under the threshold of the trip's
/// activity.
pub fn of_track(geojson: &str, activity: ActivityType) -> Option<Moving> {
    moving(
        &Profile::from_geojson(geojson)?,
        config::moving_time::min_speed_kmh(activity),
    )
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A profile of `(minute, metres along, elevation)` points.
    fn walk(points: &[(i64, f64, f64)]) -> Profile {
        Profile {
            distance_m: points.iter().map(|p| p.1).collect(),
            elevation_m: points.iter().map(|p| p.2).collect(),
            seconds: points.iter().map(|p| Some(p.0 * 60)).collect(),
        }
    }

    /// About 111 m a step, as one thousandth of a degree of latitude is.
    const STEP_M: f64 = 111.19;

    fn flat(points: &[(i64, f64)]) -> Profile {
        let points: Vec<(i64, f64, f64)> = points
            .iter()
            .map(|&(minute, steps)| (minute, steps * STEP_M, 100.0))
            .collect();
        walk(&points)
    }

    fn secs(profile: &Profile, min_speed_kmh: f64) -> Option<i64> {
        moving(profile, min_speed_kmh).map(|moving| moving.secs)
    }

    #[test]
    fn us77_time_spent_at_or_above_the_threshold_counts_as_moving() {
        // 111 m in a minute is ~6.7 km/h; the same in ten minutes ~0.67 km/h.
        let profile = flat(&[(0, 0.0), (1, 1.0), (11, 2.0)]);

        assert_eq!(secs(&profile, 1.0), Some(60));
        assert_eq!(secs(&profile, 0.5), Some(660));
        assert_eq!(secs(&profile, 7.0), Some(0));
    }

    #[test]
    fn us77_a_break_in_one_place_is_not_moving() {
        let profile = flat(&[(0, 0.0), (1, 1.0), (31, 1.0), (32, 2.0)]);

        assert_eq!(secs(&profile, 1.0), Some(120));
    }

    #[test]
    fn us77_a_track_with_fewer_than_two_timed_points_has_no_moving_time() {
        assert_eq!(secs(&flat(&[]), 1.0), None);
        assert_eq!(secs(&flat(&[(0, 0.0)]), 1.0), None);
        let mut untimed = flat(&[(0, 0.0), (1, 1.0)]);
        untimed.seconds = vec![None, None];
        assert_eq!(secs(&untimed, 1.0), None);
    }

    #[test]
    fn us77_points_at_one_instant_add_nothing() {
        let profile = flat(&[(0, 0.0), (0, 1.0), (1, 2.0)]);

        assert_eq!(secs(&profile, 1.0), Some(60));
    }

    #[test]
    fn us77_a_stored_track_is_measured_under_its_activitys_threshold() {
        // sample.gpx: two half-hour legs at ~1.6 and ~1.2 km/h.
        let track =
            crate::server::gpx::parse_gpx(include_bytes!("../../tests/fixtures/sample.gpx"))
                .unwrap();
        let blob = crate::server::geojson::build_track_geojson(&track.points);

        let secs = |activity| of_track(&blob, activity).map(|moving| moving.secs);
        assert_eq!(secs(ActivityType::Hiking), Some(3600));
        assert_eq!(secs(ActivityType::Kayaking), Some(1800));
        assert_eq!(secs(ActivityType::Cycling), Some(0));
    }

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
        let profile = flat(&[(0, 0.0), (1, 1.0), (11, 2.0)]);

        let moving = moving(&profile, 1.0).unwrap();

        assert_eq!(moving.secs, 60);
        approx(moving.distance_m, STEP_M);
        approx(
            super::moving(&profile, 0.5).unwrap().distance_m,
            2.0 * STEP_M,
        );
    }

    #[test]
    fn us80_a_break_lowers_neither_figure() {
        let without = flat(&[(0, 0.0), (1, 1.0), (2, 2.0)]);
        let with = flat(&[(0, 0.0), (1, 1.0), (31, 1.0), (32, 2.0)]);

        assert_eq!(moving(&without, 1.0), moving(&with, 1.0));
    }

    // ── US-81: climbing counts as moving ─────────────────────────────────

    #[test]
    fn us81_a_slow_steep_climb_counts_as_moving() {
        // 8 m along and 6 m up a minute: 0.48 km/h over the ground, under
        // hiking's 1 km/h, but 360 m/h up.
        let points: Vec<(i64, f64, f64)> = (0..=30)
            .map(|minute| (minute, minute as f64 * 8.0, 1000.0 + minute as f64 * 6.0))
            .collect();

        let moving = moving(&walk(&points), 1.0).unwrap();

        assert_eq!(moving.secs, 30 * 60);
        approx(moving.distance_m, 240.0);
    }

    #[test]
    fn us81_a_slow_steep_descent_counts_too() {
        let points: Vec<(i64, f64, f64)> = (0..=30)
            .map(|minute| (minute, minute as f64 * 8.0, 1000.0 - minute as f64 * 6.0))
            .collect();

        assert_eq!(secs(&walk(&points), 1.0), Some(30 * 60));
    }

    #[test]
    fn us81_a_slow_gentle_rise_is_still_standing() {
        // 1 m along and 1 m up a minute: 60 m/h, under 100 m/h.
        let points: Vec<(i64, f64, f64)> = (0..=30)
            .map(|minute| (minute, minute as f64, 1000.0 + minute as f64))
            .collect();

        assert_eq!(secs(&walk(&points), 1.0), Some(0));
    }

    #[test]
    fn us81_gps_noise_while_standing_still_is_not_climbing() {
        // A point a second for ten minutes, drifting 10 cm a second and
        // reading ±3 m of elevation: steep for a second at a time, level
        // over a minute.
        let points: Vec<(i64, f64, f64)> = (0..600)
            .map(|second| {
                let noise = if second % 2 == 0 { 3.0 } else { -3.0 };
                (second, second as f64 * 0.1, 1000.0 + noise)
            })
            .collect();
        let mut profile = walk(&points);
        profile.seconds = points.iter().map(|p| Some(p.0)).collect();

        assert_eq!(moving(&profile, 1.0).unwrap().secs, 0);
    }
}
