//! US-81 — finding climbs, on hand-made profiles (ADR-0012).

use super::*;
use crate::config::climbs::rule;

/// 10 m and 10 s a step: 3.6 km/h, moving for every activity here.
const STEP_M: f64 = 10.0;
const STEP_S: i64 = 10;

/// A profile walked at a steady pace over `segments` of (length, rise),
/// starting at 500 m, with times unless `timed` is false.
fn walk(segments: &[(f64, f64)], timed: bool) -> Profile {
    let mut profile = Profile {
        distance_m: vec![0.0],
        elevation_m: vec![500.0],
        seconds: vec![timed.then_some(0)],
    };
    for &(length, rise) in segments {
        let steps = (length / STEP_M).round() as usize;
        for _ in 0..steps {
            let last = profile.distance_m.len() - 1;
            profile.distance_m.push(profile.distance_m[last] + STEP_M);
            profile
                .elevation_m
                .push(profile.elevation_m[last] + rise / steps as f64);
            profile
                .seconds
                .push(profile.seconds[last].map(|secs| secs + STEP_S));
        }
    }
    profile
}

fn climbs(profile: &Profile, activity: ActivityType) -> Vec<Climb> {
    find(
        profile,
        rule(activity).expect("an activity with climbs"),
        crate::config::moving_time::min_speed_kmh(activity),
    )
}

fn approx(actual: f64, expected: f64, within: f64) {
    assert!(
        (actual - expected).abs() <= within,
        "{actual} is not {expected} ± {within}"
    );
}

// ── What counts as a climb ───────────────────────────────────────────────────

#[test]
fn us81_a_hill_is_a_climb_from_its_low_point_to_its_high_point() {
    let profile = walk(&[(300.0, 0.0), (1000.0, 100.0), (300.0, 0.0)], true);

    let found = climbs(&profile, ActivityType::Hiking);

    assert_eq!(found.len(), 1, "{found:?}");
    approx(found[0].gain_m, 100.0, 0.5);
    approx(found[0].start_m, 300.0, INCLINE_WINDOW_M / 2.0);
    approx(found[0].end_m, 1300.0, INCLINE_WINDOW_M / 2.0);
}

#[test]
fn us81_a_hike_counts_from_75_m() {
    let small = walk(&[(300.0, 0.0), (1000.0, 70.0), (300.0, 0.0)], true);

    assert!(climbs(&small, ActivityType::Hiking).is_empty());
}

#[test]
fn us81_a_hike_needs_no_steep_average() {
    // 85 m over 3 km, 2.8 % on average: steep, then a long gentle rise
    // within hiking's 3 km. A ride keeps only the steep part.
    let profile = walk(
        &[(300.0, 0.0), (200.0, 40.0), (2800.0, 45.0), (300.0, 0.0)],
        true,
    );

    let hike = climbs(&profile, ActivityType::Hiking);
    let ride = climbs(&profile, ActivityType::Cycling);

    assert_eq!(hike.len(), 1, "{hike:?}");
    approx(hike[0].gain_m, 85.0, 1.0);
    assert_eq!(ride.len(), 1, "{ride:?}");
    // The trim window reaches a little way past the steep part's top.
    approx(ride[0].gain_m, 40.0, 0.016 * TRIM_WINDOW_M + 0.5);
}

#[test]
fn us81_a_ride_needs_30_m_at_3_percent() {
    let steep = walk(&[(300.0, 0.0), (1000.0, 40.0), (300.0, 0.0)], true);
    let gentle = walk(&[(300.0, 0.0), (2000.0, 40.0), (300.0, 0.0)], true);
    let low = walk(&[(300.0, 0.0), (500.0, 25.0), (300.0, 0.0)], true);

    assert_eq!(climbs(&steep, ActivityType::Cycling).len(), 1);
    assert!(climbs(&gentle, ActivityType::Cycling).is_empty());
    assert!(climbs(&low, ActivityType::Cycling).is_empty());
    assert_eq!(climbs(&steep, ActivityType::Bikepacking).len(), 1);
}

#[test]
fn us81_skiing_needs_15_m_at_3_percent() {
    let profile = walk(&[(300.0, 0.0), (400.0, 20.0), (300.0, 0.0)], true);

    assert_eq!(climbs(&profile, ActivityType::CrossCountrySkiing).len(), 1);
    assert_eq!(climbs(&profile, ActivityType::SkiTouring).len(), 1);
    assert!(climbs(&profile, ActivityType::Cycling).is_empty());
}

#[test]
fn us81_only_the_activities_given_a_rule_have_climbs() {
    assert_eq!(rule(ActivityType::Kayaking), None);
    assert_eq!(rule(ActivityType::Unknown), None);
}

#[test]
fn us81_a_uniformly_gentle_walk_is_no_climb() {
    // 100 m over 10 km: nowhere as steep as 3 %.
    let profile = walk(&[(300.0, 0.0), (10_000.0, 100.0), (300.0, 0.0)], true);

    assert!(climbs(&profile, ActivityType::Hiking).is_empty());
}

// ── Where a climb ends ───────────────────────────────────────────────────────

#[test]
fn us81_a_short_dip_does_not_split_a_hill_in_two() {
    // A 5 m dip, within cycling's 10 m.
    let profile = walk(
        &[
            (300.0, 0.0),
            (500.0, 25.0),
            (200.0, -5.0),
            (500.0, 25.0),
            (300.0, 0.0),
        ],
        true,
    );

    let found = climbs(&profile, ActivityType::Cycling);

    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn us81_early_on_a_drop_beyond_the_activitys_floor_ends_the_climb() {
    // A 20 m fall after 40 m: beyond cycling's 10 m, and 10 % of 40 m is
    // less. Two climbs.
    let profile = walk(
        &[
            (300.0, 0.0),
            (1000.0, 40.0),
            (400.0, -20.0),
            (1000.0, 40.0),
            (300.0, 0.0),
        ],
        true,
    );

    let found = climbs(&profile, ActivityType::Cycling);

    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found[0].end_m < found[1].start_m,
        "in track order: {found:?}"
    );
    // Averaging lifts the sharp valley floor a little.
    approx(found[1].gain_m, 40.0, 1.0);
}

#[test]
fn us81_a_long_climb_allows_a_drop_of_a_tenth_of_its_height() {
    // 600 m up, 50 m down — within a tenth of 600 m — and 400 m up: one
    // climb, its height the rises added up.
    let profile = walk(
        &[
            (300.0, 0.0),
            (3000.0, 600.0),
            (300.0, -50.0),
            (2000.0, 400.0),
            (300.0, 0.0),
        ],
        true,
    );

    let found = climbs(&profile, ActivityType::Hiking);

    assert_eq!(found.len(), 1, "{found:?}");
    // Averaging rounds off the sharp top and floor of the drop a little.
    approx(found[0].gain_m, 1000.0, 5.0);
}

#[test]
fn us81_a_drop_deeper_than_a_tenth_of_the_height_ends_the_climb() {
    let profile = walk(
        &[
            (300.0, 0.0),
            (3000.0, 600.0),
            (500.0, -80.0),
            (2000.0, 400.0),
            (300.0, 0.0),
        ],
        true,
    );

    let found = climbs(&profile, ActivityType::Hiking);

    assert_eq!(found.len(), 2, "{found:?}");
    approx(found[0].gain_m, 600.0, 3.0);
}

#[test]
fn us81_a_climb_ends_at_its_top_not_where_the_descent_ends_it() {
    let profile = walk(&[(300.0, 0.0), (1000.0, 100.0), (1000.0, -100.0)], true);

    let found = climbs(&profile, ActivityType::Hiking);

    assert_eq!(found.len(), 1);
    approx(found[0].end_m, 1300.0, INCLINE_WINDOW_M / 2.0);
}

// ── Trimming the gentle ends ─────────────────────────────────────────────────

#[test]
fn us81_a_gentle_approach_longer_than_a_kilometre_is_cut_off() {
    // The review's case: 30 m over 3 km, then 80 m over 1 km. Kept whole,
    // it would average 2.75 % and be no ride's climb.
    let profile = walk(
        &[(300.0, 0.0), (3000.0, 30.0), (1000.0, 80.0), (300.0, 0.0)],
        true,
    );

    let found = climbs(&profile, ActivityType::Cycling);

    assert_eq!(found.len(), 1, "{found:?}");
    approx(found[0].start_m, 3300.0, TRIM_WINDOW_M);
    approx(found[0].gain_m, 80.0, 3.0);
}

#[test]
fn us81_a_gentle_approach_within_a_kilometre_is_kept() {
    let profile = walk(
        &[(300.0, 0.0), (500.0, 5.0), (1000.0, 80.0), (300.0, 0.0)],
        true,
    );

    let found = climbs(&profile, ActivityType::Cycling);

    assert_eq!(found.len(), 1);
    approx(found[0].start_m, 300.0, INCLINE_WINDOW_M);
}

#[test]
fn us81_a_plateau_beyond_a_kilometre_is_cut_off() {
    // 80 m up, then a plateau creeping 20 m higher over 5 km.
    let profile = walk(
        &[(300.0, 0.0), (1000.0, 80.0), (5000.0, 20.0), (300.0, 0.0)],
        true,
    );

    let found = climbs(&profile, ActivityType::Cycling);

    assert_eq!(found.len(), 1, "{found:?}");
    approx(found[0].end_m, 1300.0, TRIM_WINDOW_M);
    approx(found[0].gain_m, 80.0, 3.0);
}

#[test]
fn us81_a_summit_within_a_kilometre_of_flattening_is_included() {
    let profile = walk(
        &[(300.0, 0.0), (1000.0, 80.0), (600.0, 6.0), (300.0, 0.0)],
        true,
    );

    let found = climbs(&profile, ActivityType::Cycling);

    approx(found[0].end_m, 1900.0, INCLINE_WINDOW_M);
    approx(found[0].gain_m, 86.0, 1.0);
}

#[test]
fn us81_a_hikes_summit_ridge_may_run_three_kilometres() {
    let ridge = |length: f64| {
        walk(
            &[
                (300.0, 0.0),
                (1000.0, 200.0),
                (length, length / 100.0),
                (300.0, 0.0),
            ],
            true,
        )
    };

    let within = climbs(&ridge(2500.0), ActivityType::Hiking);
    let beyond = climbs(&ridge(4000.0), ActivityType::Hiking);

    approx(within[0].end_m, 3800.0, INCLINE_WINDOW_M);
    approx(beyond[0].end_m, 1300.0, TRIM_WINDOW_M);
}

// ── Moving time on a climb ───────────────────────────────────────────────────

#[test]
fn us81_a_climb_without_times_has_no_moving_time() {
    let profile = walk(&[(300.0, 0.0), (1000.0, 100.0), (300.0, 0.0)], false);

    let found = climbs(&profile, ActivityType::Hiking);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].moving_secs, None);
}

#[test]
fn us81_a_break_on_the_way_up_does_not_lower_the_rate() {
    let steady = walk(&[(300.0, 0.0), (1000.0, 100.0), (300.0, 0.0)], true);
    // The same walk with a ten-minute stop halfway up.
    let mut paused = steady.clone();
    let halfway = 80;
    paused
        .distance_m
        .insert(halfway, paused.distance_m[halfway]);
    paused
        .elevation_m
        .insert(halfway, paused.elevation_m[halfway]);
    paused.seconds.insert(halfway, paused.seconds[halfway]);
    for secs in paused.seconds.iter_mut().skip(halfway + 1).flatten() {
        *secs += 600;
    }

    let steady = climbs(&steady, ActivityType::Hiking);
    let paused = climbs(&paused, ActivityType::Hiking);

    assert_eq!(paused[0].moving_secs, steady[0].moving_secs);
    assert!(steady[0].moving_secs.unwrap() > 0);
}

// ── The trip's figures ───────────────────────────────────────────────────────

fn a_climb(gain_m: f64, moving_secs: Option<i64>) -> Climb {
    Climb {
        start_m: 0.0,
        end_m: 1000.0,
        gain_m,
        moving_secs,
    }
}

#[test]
fn us81_a_trips_figures_add_up_its_climbs_heights_and_their_moving_time() {
    let figures = totals(&[a_climb(100.0, Some(900)), a_climb(50.0, Some(300))]);

    assert_eq!(figures, (150.0, 1200));
}

#[test]
fn us81_a_climb_without_moving_time_adds_to_neither_figure() {
    let figures = totals(&[a_climb(100.0, Some(900)), a_climb(80.0, Some(0))]);

    assert_eq!(figures, (100.0, 900));
    assert_eq!(totals(&[]), (0.0, 0));
}

#[test]
fn us81_a_point_without_elevation_makes_no_climb_of_its_own() {
    // A hike up from 1500 m whose GPX lost one point's elevation, stored as
    // 0.0: read as a real reading, it would split the climb and add one.
    let points: Vec<crate::server::gpx::TrackPoint> = (0..=150)
        .map(|i| crate::server::gpx::TrackPoint {
            lat: 60.0 + i as f64 * 10.0 / 111_195.0,
            lon: 10.0,
            ele: (i != 75).then(|| 1500.0 + i.min(100) as f64 * 2.0),
            time: None,
        })
        .collect();
    let blob = crate::server::geojson::build_track_geojson(&points);

    let found = of_track(&blob, ActivityType::Hiking);

    assert_eq!(found.len(), 1, "{found:?}");
    // The track starts climbing at once, so averaging lifts its first point.
    approx(found[0].gain_m, 200.0, 3.0);
}

#[test]
fn us81_sample_gpx_rises_too_little_for_a_hike() {
    let track = crate::server::gpx::parse_gpx(include_bytes!("../../../tests/fixtures/sample.gpx"))
        .unwrap();
    let blob = crate::server::geojson::build_track_geojson(&track.points);

    assert!(of_track(&blob, ActivityType::Hiking).is_empty());
}
