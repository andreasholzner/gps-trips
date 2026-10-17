//! Synthetic trips and plans, laid out in metres east and north of 8° E,
//! 60° N: no real track of the owner's is checked in (ADR-0012).

use super::*;

/// The thresholds of these tests — fixed here, so a change to `config.rs`
/// does not move the cases they are laid out around.
const RULES: Rules = Rules {
    line_tolerance_m: 50.0,
    min_stretch_m: 200.0,
    plan_covered_share: 0.8,
    trip_on_plan_share: 0.7,
    min_covered_m: 3_000.0,
    point_tolerance_m: 100.0,
    points_reached_share: 0.8,
    min_point_run: 3,
    band_m: 300.0,
    waypoint_distance_m: 50.0,
};

const METRES_PER_DEGREE: f64 = 111_195.08;

/// `x` metres east and `y` north.
fn at(x: f64, y: f64) -> Position {
    (
        60.0 + y / METRES_PER_DEGREE,
        8.0 + x / (METRES_PER_DEGREE * 60f64.to_radians().cos()),
    )
}

/// A recorded or drawn line through `corners`, a point every `step` metres.
fn path(corners: &[(f64, f64)], step: f64) -> Vec<Position> {
    let mut points = vec![at(corners[0].0, corners[0].1)];
    for pair in corners.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        let pieces = ((x1 - x0).hypot(y1 - y0) / step).ceil().max(1.0) as usize;
        for i in 1..=pieces {
            let t = i as f64 / pieces as f64;
            points.push(at(x0 + t * (x1 - x0), y0 + t * (y1 - y0)));
        }
    }
    points
}

/// A plan of just its own points, joined by straight lines — off paths.
fn sparse(corners: &[(f64, f64)]) -> Vec<Position> {
    corners.iter().map(|&(x, y)| at(x, y)).collect()
}

fn assert_near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 0.03,
        "{actual} is not about {expected}"
    );
}

#[test]
fn us103_a_whole_stage_followed_within_a_longer_trip_is_proposed() {
    let stage = path(&[(0.0, 0.0), (10_000.0, 0.0)], 25.0);
    let trip = path(&[(-5_000.0, 10.0), (15_000.0, 10.0)], 10.0);

    let coverage = followed(&trip, &stage, &RULES).expect("followed");

    assert_eq!(coverage.judged, Judged::Line);
    assert_near(coverage.plan_covered, 1.0);
    assert_near(coverage.trip_on_plan, 0.5);
}

#[test]
fn us103_one_day_of_a_multi_day_plan_is_proposed() {
    let plan = path(&[(0.0, 0.0), (100_000.0, 0.0)], 25.0);
    let day = path(&[(20_000.0, 20.0), (45_000.0, 20.0)], 10.0);

    let coverage = followed(&day, &plan, &RULES).expect("followed");

    assert_eq!(coverage.judged, Judged::Line);
    assert_near(coverage.plan_covered, 0.25);
    assert_near(coverage.trip_on_plan, 1.0);
}

#[test]
fn us103_the_direction_of_travel_does_not_matter() {
    let stage = path(&[(0.0, 0.0), (10_000.0, 0.0)], 25.0);
    let mut trip = path(&[(-5_000.0, 10.0), (15_000.0, 10.0)], 10.0);
    trip.reverse();

    let coverage = followed(&trip, &stage, &RULES).expect("followed");

    assert_eq!(coverage.judged, Judged::Line);
    assert_near(coverage.plan_covered, 1.0);
}

#[test]
fn us103_a_trip_sharing_a_short_stretch_of_road_is_not_proposed() {
    let stage = path(&[(0.0, 0.0), (10_000.0, 0.0)], 25.0);
    let trip = path(
        &[
            (4_000.0, 10_000.0),
            (4_000.0, 20.0),
            (5_000.0, 20.0),
            (5_000.0, 10_000.0),
        ],
        10.0,
    );

    assert_eq!(followed(&trip, &stage, &RULES), None);
}

#[test]
fn us103_a_trip_crossing_a_plan_is_not_proposed() {
    let stage = path(&[(0.0, 0.0), (1_000.0, 0.0)], 25.0);
    let trip = path(&[(500.0, -5_000.0), (500.0, 5_000.0)], 10.0);

    assert_eq!(followed(&trip, &stage, &RULES), None);
}

#[test]
fn us103_a_parallel_path_beyond_the_tolerances_is_not_proposed() {
    let stage = path(&[(0.0, 0.0), (10_000.0, 0.0)], 25.0);
    let trip = path(&[(0.0, 150.0), (10_000.0, 150.0)], 10.0);

    assert_eq!(followed(&trip, &stage, &RULES), None);
}

#[test]
fn us103_an_off_path_plan_whose_points_the_trip_reached_is_proposed() {
    // Five points 2 km apart; between them the trip strays 400 m from the
    // straight lines, as a ski tour does from a plan drawn off paths.
    let plan = sparse(&[
        (0.0, 0.0),
        (2_000.0, 0.0),
        (4_000.0, 0.0),
        (6_000.0, 0.0),
        (8_000.0, 0.0),
    ]);
    let trip = path(
        &[
            (0.0, 60.0),
            (1_000.0, 400.0),
            (2_000.0, 60.0),
            (3_000.0, 400.0),
            (4_000.0, 60.0),
            (5_000.0, -400.0),
            (6_000.0, 60.0),
            (7_000.0, 400.0),
            (8_000.0, 60.0),
        ],
        10.0,
    );

    let coverage = followed(&trip, &plan, &RULES).expect("followed");

    assert_eq!(coverage.judged, Judged::Points);
    assert_near(coverage.points_reached, 1.0);
}

#[test]
fn us103_an_off_path_plan_the_trip_reached_too_little_of_is_not_proposed() {
    let plan = sparse(&[
        (0.0, 0.0),
        (2_000.0, 0.0),
        (4_000.0, 0.0),
        (6_000.0, 0.0),
        (8_000.0, 0.0),
    ]);
    let trip = path(
        &[
            (0.0, 60.0),
            (1_000.0, 400.0),
            (2_000.0, 60.0),
            (2_000.0, 5_000.0),
        ],
        10.0,
    );

    assert_eq!(followed(&trip, &plan, &RULES), None);
}

#[test]
fn us103_one_day_of_an_off_path_multi_day_plan_is_proposed() {
    // Eleven points 4 km apart; the day reaches four of them in a row and
    // stays within the band around the plan between them.
    let corners: Vec<(f64, f64)> = (0..=10).map(|i| (f64::from(i) * 4_000.0, 0.0)).collect();
    let plan = sparse(&corners);
    let day = path(
        &[
            (8_000.0, 60.0),
            (10_000.0, 200.0),
            (12_000.0, 60.0),
            (14_000.0, -200.0),
            (16_000.0, 60.0),
            (18_000.0, 200.0),
            (20_000.0, 60.0),
        ],
        10.0,
    );

    let coverage = followed(&day, &plan, &RULES).expect("followed");

    assert_eq!(coverage.judged, Judged::Points);
    assert_near(coverage.points_reached, 4.0 / 11.0);
}

#[test]
fn us103_a_longer_way_elsewhere_through_a_few_points_of_an_off_path_plan_is_not_proposed() {
    let corners: Vec<(f64, f64)> = (0..=10).map(|i| (f64::from(i) * 4_000.0, 0.0)).collect();
    let plan = sparse(&corners);
    let trip = path(
        &[
            (8_000.0, 60.0),
            (12_000.0, 60.0),
            (16_000.0, 60.0),
            (16_000.0, 20_000.0),
        ],
        10.0,
    );

    assert_eq!(followed(&trip, &plan, &RULES), None);
}

#[test]
fn us103_a_plan_too_short_to_tell_is_not_proposed() {
    let plan = path(&[(0.0, 0.0), (100.0, 0.0)], 100.0);
    let trip = path(&[(0.0, 0.0), (10_000.0, 0.0)], 10.0);

    assert_eq!(followed(&trip, &plan, &RULES), None);
}

#[test]
fn us103_lines_without_points_follow_nothing() {
    let line = path(&[(0.0, 0.0), (10_000.0, 0.0)], 25.0);

    assert_eq!(followed(&[], &line, &RULES), None);
    assert_eq!(followed(&line, &[], &RULES), None);
}

#[test]
fn us103_a_waypoint_is_passed_by_any_part_of_the_line_not_only_its_points() {
    let line = sparse(&[(0.0, 0.0), (10_000.0, 0.0)]);
    let waypoints = [at(5_000.0, 40.0), at(5_000.0, 60.0), at(0.0, -30.0)];

    assert_eq!(waypoints_passed(&line, &waypoints, &RULES), vec![0, 2]);
}

#[test]
fn us103_the_configured_rules_are_the_ones_used() {
    assert_eq!(
        Rules::CONFIGURED.line_tolerance_m,
        configured::LINE_TOLERANCE_M
    );
    assert_eq!(Rules::CONFIGURED.min_point_run, configured::MIN_POINT_RUN);
}

// Plans drawn off paths, as komoot exports them: its straight lines filled in
// with a point about every 95 m, which the trip strays well over a hundred
// metres from between the corners. Judged by the configured rules, so the
// thresholds in `config.rs` are held to these shapes.

/// A corner every 2 km along `from..to` km east, alternately 150 m north
/// and south of the plan's straight line: the way the trip went.
fn straying(from_km: i32, to_km: i32) -> Vec<(f64, f64)> {
    (from_km..=to_km)
        .map(|km| {
            let y = if km % 2 == 0 { 150.0 } else { -150.0 };
            (f64::from(km) * 1_000.0, y)
        })
        .collect()
}

#[test]
fn us103_a_coarse_off_path_plan_is_followed_by_the_configured_rules() {
    let plan = path(&[(0.0, 0.0), (5_000.0, 400.0), (10_000.0, 0.0)], 95.0);
    let trip = path(
        &[
            (0.0, 150.0),
            (2_500.0, 50.0),
            (5_000.0, 550.0),
            (7_500.0, 50.0),
            (10_000.0, -150.0),
        ],
        10.0,
    );

    let coverage = followed(&trip, &plan, &Rules::CONFIGURED).expect("followed");

    assert_eq!(coverage.judged, Judged::Points);
}

#[test]
fn us103_a_short_day_of_a_coarse_off_path_plan_is_followed_by_the_configured_rules() {
    // 50 km planned; the day covers about 2.5 km of it.
    let plan = path(&[(0.0, 0.0), (50_000.0, 0.0)], 95.0);
    let day = path(&straying(20, 22), 10.0);
    let day: Vec<Position> = day
        .into_iter()
        .chain(path(&[(22_000.0, 150.0), (22_500.0, 0.0)], 10.0))
        .collect();

    let coverage = followed(&day, &plan, &Rules::CONFIGURED).expect("followed");

    assert_eq!(coverage.judged, Judged::Points);
}
