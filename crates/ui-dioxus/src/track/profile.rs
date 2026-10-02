//! The elevation profile's derived series (US-79): how fast the owner went and
//! how steep it was, at each of the chart's samples.
//!
//! Both are derived from what the served track already carries — cumulative
//! distance, elevation and time — and smoothed here, where `cargo test`
//! reaches them; the chart only draws them (ADR-0025).

use crate::config::elevation_profile::{
    INCLINE_WINDOW_M, MIN_INCLINE_RUN_M, PAUSE_GAP_MAX_M, PAUSE_GAP_S, SPEED_WINDOW_S,
    STANDSTILL_KMH,
};
use crate::format;

use super::Track;

/// The speed in km/h at each of the chart's samples, or `None` for the whole
/// series when no point carries a time (a planned trip): there is then no
/// speed to draw and no axis to draw it against.
///
/// The speed at a point is the distance over the time between the points
/// that bracket it by half of [`SPEED_WINDOW_S`] either side, within its run
/// of timed points. A run ends where a point has no time, where time runs
/// backwards, or at a gap longer than [`PAUSE_GAP_S`]: no window reaches
/// across one. A point with no time, or alone in its run, has no speed — a
/// gap in the line rather than an invented value.
///
/// A gap is read as one stretch, and both its ends take its value: 0 km/h
/// for a pause that went nowhere, its average speed for one that did (a
/// tunnel), so the line across it is flat at what happened there. A speed
/// below [`STANDSTILL_KMH`] is drift while standing still, and reads as 0.
pub fn speed_series(track: &Track) -> Option<Vec<Option<f64>>> {
    let distance_m = chart_distances(track)?;
    let seconds: Vec<Option<f64>> = (0..distance_m.len())
        .map(|i| {
            let timestamp = track.properties.timestamps.get(i)?;
            let at = format::instant(timestamp)?;
            Some(at.unix_timestamp_nanos() as f64 / 1e9)
        })
        .collect();
    if seconds.iter().all(Option::is_none) {
        return None;
    }

    let half = SPEED_WINDOW_S / 2.0;
    let mut speeds = vec![None; distance_m.len()];
    let mut start = 0;
    while start < distance_m.len() {
        let Some(t_start) = seconds[start] else {
            start += 1;
            continue;
        };
        // The run of timed points from `start`, and its times.
        let mut times = vec![t_start];
        let mut end = start;
        while let Some(Some(next)) = seconds.get(end + 1) {
            let step = next - times[times.len() - 1];
            if !(0.0..=PAUSE_GAP_S).contains(&step) {
                break;
            }
            times.push(*next);
            end += 1;
        }
        let along = &distance_m[start..=end];
        for (i, &t) in times.iter().enumerate() {
            let before = times.partition_point(|&s| s <= t - half).saturating_sub(1);
            let after = times
                .partition_point(|&s| s < t + half)
                .min(times.len() - 1);
            speeds[start + i] =
                per_hour(along[after] - along[before], times[after] - times[before]);
        }
        start = end + 1;
    }

    // Gaps last, so each of their ends reads as the gap rather than as its
    // run; a point between two gaps reads as the one before it.
    let mut ended_a_gap = false;
    for i in 0..distance_m.len().saturating_sub(1) {
        let gap = match (seconds[i], seconds[i + 1]) {
            (Some(from), Some(to)) if to - from > PAUSE_GAP_S => Some(to - from),
            _ => None,
        };
        let Some(gap_s) = gap else {
            ended_a_gap = false;
            continue;
        };
        let gap_m = distance_m[i + 1] - distance_m[i];
        let speed = if gap_m < PAUSE_GAP_MAX_M {
            Some(0.0)
        } else {
            per_hour(gap_m, gap_s)
        };
        if !ended_a_gap {
            speeds[i] = speed;
        }
        speeds[i + 1] = speed;
        ended_a_gap = true;
    }
    Some(speeds)
}

/// The incline in percent at each of the chart's samples, positive uphill:
/// the rise over the run between the points that bracket it by half of
/// [`INCLINE_WINDOW_M`] either side — one-sided at either end of the track.
/// A run shorter than [`MIN_INCLINE_RUN_M`] has no incline.
///
/// Empty for a track that draws no chart.
pub fn inclines(track: &Track) -> Vec<Option<f64>> {
    let Some(distance_m) = chart_distances(track) else {
        return Vec::new();
    };
    let elevation_m = &track.properties.elevation_m;
    let half = INCLINE_WINDOW_M / 2.0;
    distance_m
        .iter()
        .map(|&d| {
            let before = distance_m
                .partition_point(|&m| m <= d - half)
                .saturating_sub(1);
            let after = distance_m
                .partition_point(|&m| m < d + half)
                .min(distance_m.len() - 1);
            let run = distance_m[after] - distance_m[before];
            (run >= MIN_INCLINE_RUN_M)
                .then(|| (elevation_m[after] - elevation_m[before]) / run * 100.0)
        })
        .collect()
}

/// The cumulative distances, when the track draws a chart for them to be
/// derived along (`super::elevation_series`).
fn chart_distances(track: &Track) -> Option<&[f64]> {
    let distance_m = &track.properties.cumulative_distance_m;
    (!distance_m.is_empty() && distance_m.len() == track.properties.elevation_m.len())
        .then_some(distance_m.as_slice())
}

/// Metres in seconds as km/h, with drift while standing still read as 0.
fn per_hour(metres: f64, seconds: f64) -> Option<f64> {
    if seconds <= 0.0 {
        return None;
    }
    let kmh = metres / seconds * 3.6;
    Some(if kmh < STANDSTILL_KMH { 0.0 } else { kmh })
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A track from `(seconds after the start or none, metres along)` points,
    /// all at the same elevation.
    fn timed(points: &[(Option<u32>, f64)]) -> Track {
        let timestamps: Vec<String> = points
            .iter()
            .map(|(at, _)| at.map_or_else(String::new, at_second))
            .collect();
        let distances: Vec<f64> = points.iter().map(|&(_, m)| m).collect();
        track(&distances, &vec![0.0; points.len()], &timestamps)
    }

    /// A track from `(metres along, metres up)` points, without times.
    fn hilly(points: &[(f64, f64)]) -> Track {
        let distances: Vec<f64> = points.iter().map(|&(m, _)| m).collect();
        let elevations: Vec<f64> = points.iter().map(|&(_, e)| e).collect();
        track(&distances, &elevations, &vec![String::new(); points.len()])
    }

    fn track(distances: &[f64], elevations: &[f64], timestamps: &[String]) -> Track {
        serde_json::from_value(serde_json::json!({
            "geometry": {"coordinates": []},
            "properties": {
                "cumulative_distance_m": distances,
                "elevation_m": elevations,
                "timestamps": timestamps,
            }
        }))
        .unwrap()
    }

    fn at_second(second: u32) -> String {
        format!(
            "2026-07-11T{:02}:{:02}:{:02}Z",
            8 + second / 3600,
            second / 60 % 60,
            second % 60
        )
    }

    /// One point a second for `seconds`, at `metres_per_second`.
    fn steady(seconds: u32, metres_per_second: f64) -> Vec<(Option<u32>, f64)> {
        (0..=seconds)
            .map(|s| (Some(s), f64::from(s) * metres_per_second))
            .collect()
    }

    fn speeds(track: &Track) -> Vec<Option<f64>> {
        speed_series(track).expect("a timed track has a speed series")
    }

    fn approx(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("a speed or incline");
        assert!(
            (actual - expected).abs() < 1e-6,
            "{actual} is not {expected}"
        );
    }

    // ── Speed ────────────────────────────────────────────────────────────

    #[test]
    fn a_steady_pace_reads_as_its_speed_everywhere() {
        // 5 m/s is 18 km/h, at the ends as much as in the middle.
        let speeds = speeds(&timed(&steady(120, 5.0)));

        assert_eq!(speeds.len(), 121);
        for speed in speeds {
            approx(speed, 18.0);
        }
    }

    #[test]
    fn jitter_between_points_a_second_apart_is_smoothed_away() {
        // Alternate seconds of 2 m and 8 m: 18 km/h and 7.2 km/h raw, 18 km/h
        // on average — and the window sees the average.
        let mut along = 0.0;
        let points: Vec<_> = (0..=120)
            .map(|s| {
                along += match s {
                    0 => 0.0,
                    s if s % 2 == 0 => 2.0,
                    _ => 8.0,
                };
                (Some(s), along)
            })
            .collect();

        let speeds = speeds(&timed(&points));

        let middle = speeds[60].unwrap();
        assert!((middle - 18.0).abs() < 1.0, "{middle}");
    }

    #[test]
    fn drift_while_standing_still_reads_as_a_pause() {
        // 0.1 m/s is 0.36 km/h: under the standstill floor.
        let speeds = speeds(&timed(&steady(120, 0.1)));

        assert_eq!(speeds[60], Some(0.0));
    }

    #[test]
    fn a_slow_scramble_still_reads_as_moving() {
        // 0.25 m/s is 0.9 km/h: slow, but above the floor.
        let speeds = speeds(&timed(&steady(120, 0.25)));

        approx(speeds[60], 0.9);
    }

    #[test]
    fn a_long_gap_that_went_nowhere_is_a_pause_at_both_its_ends() {
        // Ten minutes with the recording stopped, 20 m further on.
        let mut points = steady(60, 5.0);
        points.extend(
            steady(60, 5.0)
                .into_iter()
                .map(|(s, m)| (s.map(|s| s + 660), m + 320.0)),
        );

        let speeds = speeds(&timed(&points));

        assert_eq!(speeds[60], Some(0.0));
        assert_eq!(speeds[61], Some(0.0));
        approx(speeds[30], 18.0);
        approx(speeds[91], 18.0);
    }

    #[test]
    fn a_long_gap_that_went_somewhere_reads_as_its_average_at_both_its_ends() {
        // A tunnel without reception: two minutes, 1200 m — 36 km/h.
        let mut points = steady(60, 5.0);
        points.extend(
            steady(60, 5.0)
                .into_iter()
                .map(|(s, m)| (s.map(|s| s + 180), m + 1500.0)),
        );

        let speeds = speeds(&timed(&points));

        approx(speeds[60], 36.0);
        approx(speeds[61], 36.0);
    }

    #[test]
    fn no_window_reaches_across_a_gap() {
        // Five seconds before a tunnel the window would reach into it, and
        // read faster than the 18 km/h the owner was going.
        let mut points = steady(60, 5.0);
        points.extend(
            steady(60, 5.0)
                .into_iter()
                .map(|(s, m)| (s.map(|s| s + 180), m + 1500.0)),
        );

        let speeds = speeds(&timed(&points));

        approx(speeds[55], 18.0);
        approx(speeds[66], 18.0);
    }

    #[test]
    fn a_stretch_without_times_has_no_speed() {
        let mut points = steady(60, 5.0);
        for point in &mut points[20..30] {
            point.0 = None;
        }

        let speeds = speeds(&timed(&points));

        assert_eq!(speeds[25], None);
        approx(speeds[10], 18.0);
        approx(speeds[40], 18.0);
    }

    #[test]
    fn a_point_whose_time_runs_backwards_has_no_speed() {
        let speeds = speeds(&timed(&[(Some(10), 0.0), (Some(5), 50.0)]));

        assert_eq!(speeds, vec![None, None]);
    }

    #[test]
    fn a_track_without_times_has_no_speed_series() {
        assert_eq!(speed_series(&timed(&[(None, 0.0), (None, 50.0)])), None);
    }

    #[test]
    fn a_track_that_draws_no_chart_has_no_speed_series() {
        let lopsided = track(&[0.0, 1.0], &[12.0], &[at_second(0), at_second(1)]);

        assert_eq!(speed_series(&lopsided), None);
    }

    // ── Incline ──────────────────────────────────────────────────────────

    #[test]
    fn uphill_is_positive_and_downhill_negative() {
        // 10 m points: up 1 m each for 100 m, then down 2 m each.
        let mut points: Vec<_> = (0..=10)
            .map(|i| (f64::from(i) * 10.0, f64::from(i)))
            .collect();
        points.extend((1..=10).map(|i| (100.0 + f64::from(i) * 10.0, 10.0 - 2.0 * f64::from(i))));

        let inclines = inclines(&hilly(&points));

        approx(inclines[5], 10.0);
        approx(inclines[15], -20.0);
    }

    #[test]
    fn the_incline_is_smoothed_over_its_window() {
        // A 1 m step every 10 m on a staircase of flat 10 m treads reads as
        // the 10 % slope it climbs, not as alternating 0 % and a cliff.
        let points: Vec<_> = (0..=20)
            .map(|i| (f64::from(i) * 5.0, f64::from(i / 2)))
            .collect();

        let inclines = inclines(&hilly(&points));

        approx(inclines[10], 10.0);
    }

    #[test]
    fn the_incline_at_either_end_is_measured_on_its_one_side() {
        let points: Vec<_> = (0..=10)
            .map(|i| (f64::from(i) * 10.0, f64::from(i)))
            .collect();

        let inclines = inclines(&hilly(&points));

        approx(inclines[0], 10.0);
        approx(inclines[10], 10.0);
    }

    #[test]
    fn standing_still_has_no_incline() {
        let inclines = inclines(&hilly(&[(0.0, 10.0), (0.5, 12.0)]));

        assert_eq!(inclines, vec![None, None]);
    }

    #[test]
    fn a_track_without_elevation_is_flat() {
        // The server writes zeroes for a GPX without elevation; the profile
        // is a flat line, and the incline agrees with it.
        let inclines = inclines(&hilly(&[(0.0, 0.0), (100.0, 0.0)]));

        assert_eq!(inclines, vec![Some(0.0), Some(0.0)]);
    }

    #[test]
    fn a_track_that_draws_no_chart_has_no_inclines() {
        let lopsided = track(&[0.0, 1.0], &[12.0], &[]);

        assert_eq!(inclines(&lopsided), Vec::new());
    }
}
