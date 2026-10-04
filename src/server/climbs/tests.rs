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

// ── Smoothing ────────────────────────────────────────────────────────────────

#[test]
fn us81_the_elevation_is_averaged_over_the_incline_window() {
    // A steady 10 % slope with ±3 m of jitter on every other point.
    let mut profile = walk(&[(1000.0, 100.0)], true);
    for (i, elevation) in profile.elevation_m.iter_mut().enumerate() {
        *elevation += if i % 2 == 0 { 3.0 } else { -3.0 };
    }

    let smoothed = smoothed(&profile.distance_m, &profile.elevation_m);

    // Mid-slope, the slope stays and the jitter shrinks to a fifth: the
    // window holds five points, which never cancel out entirely.
    approx(smoothed[50], 550.0, 3.0 / 5.0 + 1e-9);
    approx(smoothed[51], 551.0, 3.0 / 5.0 + 1e-9);
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
fn us81_a_hike_counts_from_75_m_whatever_its_gradient() {
    let small = walk(&[(300.0, 0.0), (1000.0, 70.0), (300.0, 0.0)], true);
    let long = walk(&[(300.0, 0.0), (5000.0, 80.0), (300.0, 0.0)], true);

    assert!(climbs(&small, ActivityType::Hiking).is_empty());
    assert_eq!(climbs(&long, ActivityType::Hiking).len(), 1);
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
    approx(found[0].gain_m, 45.0, 0.5);
}

#[test]
fn us81_a_dip_deeper_than_the_activitys_allowance_ends_the_climb() {
    // A 20 m fall, beyond cycling's 10 m: two climbs.
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
fn us81_only_the_activities_given_a_rule_have_climbs() {
    assert_eq!(rule(ActivityType::Kayaking), None);
    assert_eq!(rule(ActivityType::Unknown), None);
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
fn us81_a_stored_track_is_read_back_into_its_profile() {
    let track = crate::server::gpx::parse_gpx(include_bytes!("../../../tests/fixtures/sample.gpx"))
        .unwrap();
    let blob = crate::server::geojson::build_track_geojson(&track.points);

    let profile = Profile::from_geojson(&blob).expect("a profile");

    assert_eq!(profile.distance_m.len(), track.points.len());
    assert_eq!(profile.elevation_m.len(), track.points.len());
    assert!(profile.seconds.iter().all(Option::is_some));
    // sample.gpx rises 40 m: no hike's climb.
    assert!(of_track(&blob, ActivityType::Hiking).is_empty());
}
