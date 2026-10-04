//! The naming rules over synthetic places, laid out in metres around a
//! point at 60° N (ADR-0012).

use geo::{Coord, LineString, MultiPolygon, Polygon};

use geo::Contains;

use super::place_part;
use crate::server::gpx::TrackPoint;
use crate::server::places::{Place, PlaceKind, Shape, Source};

const ORIGIN: Coord = Coord { x: 8.0, y: 60.0 };

/// The longitude/latitude `x` metres east and `y` metres north of the
/// origin.
fn at(x: f64, y: f64) -> Coord {
    Coord {
        x: ORIGIN.x + x / (111_320.0 * ORIGIN.y.to_radians().cos()),
        y: ORIGIN.y + y / 110_574.0,
    }
}

/// A track through `waypoints` (metres), a point every 50 m between them.
fn track(waypoints: &[(f64, f64)]) -> Vec<TrackPoint> {
    let waypoints: Vec<(f64, f64, f64)> = waypoints.iter().map(|&(x, y)| (x, y, 0.0)).collect();
    track_3d(&waypoints)
}

/// A track through `waypoints` (metres east, north and up), a point every
/// 50 m between them.
fn track_3d(waypoints: &[(f64, f64, f64)]) -> Vec<TrackPoint> {
    let point = |x: f64, y: f64, ele: f64| {
        let c = at(x, y);
        TrackPoint {
            lat: c.y,
            lon: c.x,
            ele: Some(ele),
            time: None,
        }
    };
    let (x, y, ele) = waypoints[0];
    let mut points = vec![point(x, y, ele)];
    for pair in waypoints.windows(2) {
        let ((x0, y0, e0), (x1, y1, e1)) = (pair[0], pair[1]);
        let steps = ((x1 - x0).hypot(y1 - y0) / 50.0).ceil().max(1.0) as usize;
        for step in 1..=steps {
            let t = step as f64 / steps as f64;
            points.push(point(
                x0 + (x1 - x0) * t,
                y0 + (y1 - y0) * t,
                e0 + (e1 - e0) * t,
            ));
        }
    }
    points
}

fn coords(points: &[TrackPoint]) -> Vec<Coord> {
    points
        .iter()
        .map(|p| Coord { x: p.lon, y: p.lat })
        .collect()
}

fn point(name: &str, kind: PlaceKind, x: f64, y: f64) -> Place {
    Place {
        name: name.to_string(),
        kind,
        source: Source::Osm,
        shape: Shape::Point(at(x, y)),
        ele_m: None,
        prominence_m: None,
        area_m2: None,
        population: None,
    }
}

fn summit(name: &str, ele_m: f64, x: f64, y: f64) -> Place {
    Place {
        ele_m: Some(ele_m),
        ..point(name, PlaceKind::Summit, x, y)
    }
}

/// A square area `side` metres across, centred at (`x`, `y`).
fn area(name: &str, kind: PlaceKind, x: f64, y: f64, side: f64) -> Place {
    let h = side / 2.0;
    let ring = LineString(vec![
        at(x - h, y - h),
        at(x + h, y - h),
        at(x + h, y + h),
        at(x - h, y + h),
        at(x - h, y - h),
    ]);
    Place {
        shape: Shape::Area(MultiPolygon(vec![Polygon::new(ring, vec![])])),
        area_m2: Some(side * side),
        ..point(name, kind, x, y)
    }
}

// ── A trip that ends somewhere else ─────────────────────────────────────────

#[test]
fn us74_a_one_way_trip_reads_start_dash_end() {
    let places = vec![
        point("Rysstad", PlaceKind::Village, 0.0, 200.0),
        area("Kilefjorden", PlaceKind::Bay, 10_000.0, 400.0, 600.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Rysstad - Kilefjorden")
    );
}

#[test]
fn us74_an_end_is_named_after_the_most_important_place_in_reach() {
    // The hamlet is closer, but the town reaches farther and weighs more.
    let places = vec![
        point("Start", PlaceKind::Village, 0.0, 0.0),
        point("Grenda", PlaceKind::Hamlet, 10_000.0, 200.0),
        point("Byen", PlaceKind::Town, 10_000.0, 1_500.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Start - Byen")
    );
}

#[test]
fn us74_a_place_out_of_its_reach_does_not_name_an_end() {
    let places = vec![
        point("Start", PlaceKind::Village, 0.0, 0.0),
        point("Grenda", PlaceKind::Hamlet, 10_000.0, 300.0),
        point("Byen", PlaceKind::Town, 10_000.0, 4_000.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Start - Grenda")
    );
}

#[test]
fn us74_a_lake_counts_by_its_shoreline_not_its_centre() {
    // The lake's centre is 1.1 km from the end, its shore 100 m.
    let places = vec![
        point("Start", PlaceKind::Village, 0.0, 0.0),
        area("Storvatnet", PlaceKind::Lake, 10_000.0, 1_100.0, 2_000.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Start - Storvatnet")
    );
}

#[test]
fn us74_a_campsite_at_an_end_gives_the_name_of_the_place_it_is_named_after() {
    let places = vec![
        point("Andenes", PlaceKind::Town, 0.0, 0.0),
        point("Fjordbotn Camping", PlaceKind::Campsite, 10_000.0, 50.0),
        // In reach and weightier, but the trip stopped at the campsite.
        point("Bleik", PlaceKind::Village, 10_000.0, 1_000.0),
        point("Fjordbotn", PlaceKind::Farm, 12_000.0, 0.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Andenes - Fjordbotn")
    );
}

#[test]
fn us74_a_hut_named_after_nothing_gives_its_own_name() {
    let places = vec![
        point("Hovden", PlaceKind::Village, 0.0, 0.0),
        point("Lislefjødd", PlaceKind::Hut, 10_000.0, 50.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Hovden - Lislefjødd")
    );
}

#[test]
fn us74_without_names_at_the_ends_the_main_places_stand_in_track_order() {
    let places = vec![
        summit("884", 884.0, 9_000.0, 50.0),
        summit("Leirholtinden", 1_100.0, 2_000.0, 0.0),
        summit("Storsteinnestinden", 1_200.0, 5_000.0, -100.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Leirholtinden - Storsteinnestinden - 884")
    );
}

#[test]
fn us74_only_the_weightiest_main_places_are_named() {
    let places = vec![
        summit("A", 1_000.0, 1_000.0, 0.0),
        summit("B", 500.0, 3_000.0, 0.0),
        summit("C", 1_200.0, 5_000.0, 0.0),
        summit("D", 1_100.0, 7_000.0, 0.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("A - C - D")
    );
}

#[test]
fn us74_a_place_the_track_does_not_pass_close_to_is_not_a_main_place() {
    let places = vec![
        summit("Passed", 900.0, 3_000.0, 100.0),
        summit("Seen", 2_000.0, 6_000.0, 400.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Passed")
    );
}

#[test]
fn us74_farms_and_localities_never_count() {
    let places = vec![
        point("Gården", PlaceKind::Farm, 0.0, 0.0),
        point("Stedet", PlaceKind::Locality, 5_000.0, 0.0),
        point("Gården 2", PlaceKind::Farm, 10_000.0, 0.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places),
        None
    );
}

#[test]
fn us74_one_named_end_is_joined_by_the_main_places() {
    let places = vec![
        point("Start", PlaceKind::Village, 0.0, 0.0),
        summit("Toppen", 1_000.0, 6_000.0, 0.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Start - Toppen")
    );
}

#[test]
fn us74_ends_named_alike_make_a_round_trip() {
    // 3 km apart, both in the city.
    let places = vec![
        point("Tromsø", PlaceKind::City, 1_500.0, 0.0),
        summit("Fløya", 671.0, 1_500.0, 4_000.0),
    ];
    assert_eq!(
        place_part(
            &track(&[(0.0, 0.0), (1_500.0, 4_000.0), (3_000.0, 0.0)]),
            places
        )
        .as_deref(),
        Some("Tromsø: Fløya")
    );
}

#[test]
fn us74_no_place_found_is_no_suggestion() {
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), vec![]),
        None
    );
    assert_eq!(place_part(&[], vec![]), None);
}

// ── A round trip ────────────────────────────────────────────────────────────

/// Out to 8 km east and back, ending 300 m from the start.
fn out_and_back() -> Vec<TrackPoint> {
    track(&[(0.0, 0.0), (8_000.0, 0.0), (8_000.0, 500.0), (0.0, 300.0)])
}

#[test]
fn us74_a_round_trip_reads_its_start_then_its_main_places() {
    let places = vec![
        point("Tromsø", PlaceKind::City, 0.0, 1_000.0),
        summit("Nær", 600.0, 5_000.0, 0.0),
        summit("Fjern", 600.0, 7_000.0, 500.0),
    ];
    // The one near the turning point weighs more, but not three times the
    // other, so both stand.
    assert_eq!(
        place_part(&out_and_back(), places).as_deref(),
        Some("Tromsø: Nær - Fjern")
    );
}

#[test]
fn us74_on_a_round_trip_the_place_nearest_the_turning_point_weighs_most() {
    // Four of a height: those near the start weigh under a third of the
    // one at the turning point, and are left out.
    let places = vec![
        summit("A", 600.0, 1_000.0, 0.0),
        summit("B", 600.0, 2_000.0, 0.0),
        summit("C", 600.0, 6_000.0, 0.0),
        summit("D", 600.0, 7_500.0, 490.0),
    ];
    assert_eq!(
        place_part(&out_and_back(), places).as_deref(),
        Some("C - D")
    );
}

#[test]
fn us74_a_round_trip_s_one_dominant_place_is_named_alone() {
    let places = vec![
        point("Langryggen", PlaceKind::Locality, 0.0, 0.0),
        summit("Liten", 300.0, 2_000.0, 0.0),
        Place {
            prominence_m: Some(800.0),
            ..summit("Hamperokken", 1_404.0, 8_000.0, 250.0)
        },
        summit("Liten 2", 300.0, 4_000.0, 500.0),
    ];
    assert_eq!(
        place_part(&out_and_back(), places).as_deref(),
        Some("Langryggen: Hamperokken")
    );
}

#[test]
fn us74_a_round_trip_names_its_turning_point_over_a_place_near_the_start() {
    let places = vec![
        point("Tromsø", PlaceKind::City, 0.0, 0.0),
        area("Kvaløyvågen", PlaceKind::Bay, 8_200.0, 250.0, 300.0),
        summit("Nærtoppen", 150.0, 1_000.0, 0.0),
    ];
    assert_eq!(
        place_part(&out_and_back(), places).as_deref(),
        Some("Tromsø: Kvaløyvågen")
    );
}

#[test]
fn us74_a_hike_turns_at_its_summit_rather_than_its_farthest_point() {
    // Up 300 m to a summit 1 km out, then on along the flat to a lake 2 km
    // out and back: the summit is where the effort turned.
    let hike = track_3d(&[
        (0.0, 0.0, 100.0),
        (1_000.0, 0.0, 400.0),
        (1_200.0, 0.0, 100.0),
        (2_000.0, 0.0, 100.0),
        (2_000.0, 200.0, 100.0),
        (0.0, 200.0, 100.0),
    ]);
    let places = vec![
        summit("Toppen", 400.0, 1_000.0, 0.0),
        area("Vatnet", PlaceKind::Lake, 2_000.0, 300.0, 120.0),
    ];
    assert_eq!(place_part(&hike, places).as_deref(), Some("Toppen"));
}

#[test]
fn us74_a_round_trip_without_a_named_start_is_its_main_places() {
    let places = vec![summit("Toppen", 900.0, 8_000.0, 250.0)];
    assert_eq!(
        place_part(&out_and_back(), places).as_deref(),
        Some("Toppen")
    );
}

#[test]
fn us74_a_round_trip_does_not_name_its_start_twice() {
    let places = vec![area("Vatnet", PlaceKind::Lake, 0.0, -400.0, 600.0)];
    assert_eq!(
        place_part(&out_and_back(), places).as_deref(),
        Some("Vatnet")
    );
}

// ── Names ───────────────────────────────────────────────────────────────────

#[test]
fn us74_a_place_named_twice_counts_once() {
    let places = vec![
        summit("Kleivtoppen", 700.0, 5_000.0, 0.0),
        summit("Kleivtoppen", 700.0, 5_100.0, 0.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Kleivtoppen")
    );
}

fn in_register(place: Place) -> Place {
    Place {
        source: Source::Kartverket,
        ..place
    }
}

#[test]
fn us74_the_register_s_name_wins_and_its_place_counts_once() {
    let places = vec![
        summit("Stuora Gáranasgáisi", 1_404.0, 5_000.0, 0.0),
        in_register(point("Hamperokken", PlaceKind::Summit, 5_030.0, 0.0)),
        summit("Liten", 300.0, 7_000.0, 0.0),
    ];
    // Both summits named: the merged one keeps OSM's height and outweighs
    // the other.
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Hamperokken - Liten")
    );
}

#[test]
fn us74_a_register_s_lake_takes_osm_s_outline() {
    let places = vec![
        point("Start", PlaceKind::Village, 0.0, 0.0),
        // The register's lake is a point 1.1 km from the end; OSM's outline
        // around it reaches to 100 m.
        in_register(point("Storvatnet", PlaceKind::Lake, 10_000.0, 1_100.0)),
        area("Store vatn", PlaceKind::Lake, 10_000.0, 1_100.0, 2_000.0),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Start - Storvatnet")
    );
}

#[test]
fn us74_differently_named_places_of_two_sources_apart_are_two_places() {
    let places = vec![
        summit("Nordtoppen", 900.0, 5_000.0, 0.0),
        in_register(point("Sørtoppen", PlaceKind::Summit, 5_000.0, 120.0)),
    ];
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Nordtoppen - Sørtoppen")
    );
}

#[test]
fn us74_two_places_of_one_name_far_apart_are_two_places() {
    let places = vec![
        area("Langvatnet", PlaceKind::Lake, 2_000.0, 250.0, 400.0),
        area("Langvatnet", PlaceKind::Lake, 8_000.0, 250.0, 400.0),
    ];
    // Still named once: a name says where a trip went, not how often.
    assert_eq!(
        place_part(&track(&[(0.0, 0.0), (10_000.0, 0.0)]), places).as_deref(),
        Some("Langvatnet")
    );
}

// ── Looking places up ───────────────────────────────────────────────────────

#[test]
fn us74_places_are_looked_up_stretch_by_stretch_as_far_as_any_reaches() {
    let coords = coords(&track(&[(0.0, 0.0), (30_000.0, 0.0), (30_000.0, 30_000.0)]));

    let boxes = super::lookup_boxes(&coords);

    // Long enough for several stretches, not one box over the whole corner.
    assert!(boxes.len() > 1, "{boxes:?}");
    assert!(!boxes.iter().any(|b| b.contains(&at(15_000.0, 15_000.0))));
    // Every point, and the farthest reach around it, is covered.
    let reach = super::LOOKUP_REACH_M - 1.0;
    for c in [
        at(0.0, -reach),
        at(30_000.0 + reach, 30_000.0),
        at(15_000.0, reach),
    ] {
        assert!(boxes.iter().any(|b| b.contains(&c)), "{c:?} not covered");
    }
    assert!(super::lookup_boxes(&[]).is_empty());
}
