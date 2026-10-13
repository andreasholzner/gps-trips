//! US-73 — which trips the zoomed-in map draws, and how (ADR-0012).

use super::*;
use crate::heat;
use trip_archive_types::{TripKind, TripSummary, TripTrack};

/// A trip whose box is `[west, south, east, north]`, or none.
fn trip(id: i64, activity_type: ActivityType, bbox: Option<[f64; 4]>) -> MapItem {
    let [min_lon, min_lat, max_lon, max_lat] = match bbox {
        Some(corners) => corners.map(Some),
        None => [None; 4],
    };
    MapItem::from(&TripSummary {
        id,
        name: format!("Trip {id}"),
        activity_type,
        start_time: None,
        start_date: None,
        distance_m: 1_000.0,
        ascent_m: None,
        duration_secs: None,
        trip_kind: TripKind::Recorded,
        privacy_status: None,
        min_lat,
        min_lon,
        max_lat,
        max_lon,
    })
}

fn hike(id: i64, bbox: [f64; 4]) -> MapItem {
    trip(id, ActivityType::Hiking, Some(bbox))
}

/// Trip `id`'s track as `GET /api/trips/tracks` answers it: `[lon, lat]`.
fn a_track(id: i64) -> TripTrack {
    TripTrack {
        id,
        coordinates: vec![[10.7, 59.9], [10.8, 60.0]],
    }
}

fn view(bounds: [f64; 4]) -> Viewport {
    Viewport { zoom: 12.0, bounds }
}

fn ids(trips: &[&MapItem]) -> Vec<i64> {
    trips.iter().map(|trip| trip.id).collect()
}

#[test]
fn the_map_reports_where_it_is_looking() {
    let viewport: Viewport = serde_json::from_value(serde_json::json!({
        "zoom": 11, "bounds": [10.0, 59.0, 11.0, 60.0]
    }))
    .unwrap();

    assert_eq!(
        viewport,
        Viewport {
            zoom: 11.0,
            bounds: [10.0, 59.0, 11.0, 60.0]
        }
    );
}

#[test]
fn lines_are_drawn_from_the_threshold_on() {
    let at = |zoom| Viewport {
        zoom,
        bounds: [0.0; 4],
    };

    assert!(!shows_lines(&at(LINES_FROM_ZOOM - 1.0)));
    assert!(shows_lines(&at(LINES_FROM_ZOOM)));
    assert!(shows_lines(&at(LINES_FROM_ZOOM + 5.0)));
}

#[test]
fn only_trips_whose_box_overlaps_the_view_are_in_view() {
    let trips = [
        hike(1, [10.5, 59.5, 10.6, 59.6]), // inside
        hike(2, [9.0, 59.5, 10.2, 59.6]),  // across the west edge
        hike(3, [12.0, 59.5, 13.0, 59.6]), // east of the view
        hike(4, [10.5, 61.0, 10.6, 62.0]), // north of the view
        hike(5, [9.0, 58.0, 12.0, 61.0]),  // around the whole view
    ];

    let shown = in_view(&trips, &view([10.0, 59.0, 11.0, 60.0]));

    assert_eq!(ids(&shown), [1, 2, 5]);
}

#[test]
fn a_box_touching_the_views_edge_is_in_view() {
    let trips = [
        hike(1, [11.0, 59.5, 11.5, 59.6]),
        hike(2, [10.5, 58.0, 10.6, 59.0]),
    ];

    assert_eq!(
        ids(&in_view(&trips, &view([10.0, 59.0, 11.0, 60.0]))),
        [1, 2]
    );
}

#[test]
fn a_trip_without_a_box_is_never_in_view() {
    let trips = [trip(1, ActivityType::Hiking, None)];

    assert!(in_view(&trips, &view([-180.0, -90.0, 180.0, 90.0])).is_empty());
}

#[test]
fn a_view_across_the_antimeridian_sees_both_sides() {
    let trips = [
        hike(1, [178.0, -17.0, 179.0, -16.0]), // Fiji, west of the line
        hike(2, [-179.5, -17.0, -179.0, -16.0]), // east of it
        hike(3, [170.0, -17.0, 171.0, -16.0]), // out of view
    ];

    let east_of_180 = in_view(&trips, &view([177.0, -18.0, 181.5, -15.0]));
    let west_of_minus_180 = in_view(&trips, &view([-183.0, -18.0, -178.5, -15.0]));

    assert_eq!(ids(&east_of_180), [1, 2]);
    assert_eq!(ids(&west_of_minus_180), [1, 2]);
}

#[test]
fn each_line_is_its_trips_shade_in_list_order_over_every_matching_trip() {
    // Trip 1 is out of view and has no track fetched, yet it keeps the
    // first hiking shade: panning must not recolor trip 3.
    let trips = [
        hike(1, [0.0, 0.0, 1.0, 1.0]),
        trip(2, ActivityType::Cycling, Some([10.0, 59.0, 11.0, 60.0])),
        hike(3, [10.0, 59.0, 11.0, 60.0]),
    ];

    let drawn = lines(
        &trips,
        &[
            (MapSource::Trips, &a_track(2)),
            (MapSource::Trips, &a_track(3)),
        ],
        &heat::marks(&trips),
    );

    let drawn: Vec<_> = drawn
        .lines
        .iter()
        .map(|line| (line.id, line.color))
        .collect();
    assert_eq!(drawn, [(2, "#1f4e9c"), (3, "#9b2543")]);
}

#[test]
fn a_line_is_named_after_its_trip_and_follows_its_track() {
    let trips = [hike(7, [10.0, 59.0, 11.0, 60.0])];

    let drawn = lines(
        &trips,
        &[(MapSource::Trips, &a_track(7))],
        &heat::marks(&trips),
    );

    assert_eq!(drawn.lines[0].name, "Trip 7");
    assert_eq!(drawn.lines[0].points, vec![[59.9, 10.7], [60.0, 10.8]]);
}

#[test]
fn a_track_that_could_not_be_read_is_left_off() {
    let trips = [
        hike(1, [10.0, 59.0, 11.0, 60.0]),
        hike(2, [10.0, 59.0, 11.0, 60.0]),
    ];

    let drawn = lines(
        &trips,
        &[(MapSource::Trips, &a_track(2))],
        &heat::marks(&trips),
    );

    assert_eq!(drawn.lines.len(), 1);
    assert_eq!(drawn.lines[0].id, 2);
}

#[test]
fn fit_to_trips_still_covers_every_matching_trip() {
    let trips = [
        hike(1, [0.0, 0.0, 2.0, 2.0]),
        hike(2, [10.0, 59.0, 12.0, 61.0]),
    ];

    let drawn = lines(
        &trips,
        &[(MapSource::Trips, &a_track(2))],
        &heat::marks(&trips),
    );

    assert_eq!(drawn.fit, vec![[1.0, 1.0], [60.0, 11.0]]);
}

#[test]
fn the_legend_names_the_activities_of_the_lines_drawn() {
    let trips = [
        trip(1, ActivityType::Kayaking, Some([0.0, 0.0, 1.0, 1.0])),
        hike(2, [10.0, 59.0, 11.0, 60.0]),
    ];

    let drawn = lines(
        &trips,
        &[(MapSource::Trips, &a_track(2))],
        &heat::marks(&trips),
    );

    assert_eq!(activities(&trips, &drawn), [ActivityType::Hiking]);
}

#[test]
fn only_the_tracks_not_yet_read_are_asked_for() {
    let mut cache = TrackCache::default();
    cache.store(&[1, 2], vec![a_track(1), a_track(2)]);

    assert_eq!(cache.missing(&[1, 2, 3, 4]), [3, 4]);
    assert!(cache.missing(&[2, 1]).is_empty());
}

#[test]
fn a_track_the_archive_did_not_send_is_not_asked_for_again() {
    // Trip 2 has no track, or is gone: asking again would get nothing again.
    let mut cache = TrackCache::default();
    cache.store(&[1, 2], vec![a_track(1)]);

    assert!(cache.missing(&[1, 2]).is_empty());
    assert!(cache.get(1).is_some());
    assert_eq!(cache.get(2), None);
}

#[test]
fn the_cache_hands_back_a_track_it_read() {
    let mut cache = TrackCache::default();
    cache.store(&[1, 2, 3], vec![a_track(1), a_track(2), a_track(3)]);

    assert_eq!(cache.get(3), Some(&a_track(3)));
    assert_eq!(cache.get(4), None, "never asked for");
}

#[test]
fn a_track_already_asked_for_is_not_asked_for_again_while_it_loads() {
    let pending = std::collections::HashSet::from([2, 3]);

    assert_eq!(to_request(&[1, 2, 3, 4], &pending), [1, 4]);
}

#[test]
fn us87_zoomed_in_the_map_is_still_fitted_to_what_the_list_adds() {
    let trips = [hike(1, [0.0, 0.0, 2.0, 2.0])];
    let marks = heat::marks(&trips).also_fitting(&[[46.5, 9.2]]);

    let drawn = lines(&trips, &[], &marks);

    assert_eq!(drawn.fit, vec![[1.0, 1.0], [46.5, 9.2]]);
}

// ── US-90: every kind on one map ─────────────────────────────────────────────

fn of_kind(kind: crate::kinds::Kind, id: i64) -> MapItem {
    MapItem {
        kind,
        ..hike(id, [10.0, 59.0, 11.0, 60.0])
    }
}

#[test]
fn us90_each_line_is_drawn_in_its_kinds_style() {
    use crate::kinds::Kind;
    let items = [
        of_kind(Kind::Recorded, 1),
        of_kind(Kind::Planned, 2),
        of_kind(Kind::Collections, 3),
    ];

    let drawn = lines(
        &items,
        &[
            (MapSource::Trips, &a_track(1)),
            (MapSource::Trips, &a_track(2)),
            (MapSource::CollectionTracks, &a_track(3)),
        ],
        &heat::marks(&items),
    );

    let styles: Vec<_> = drawn
        .lines
        .iter()
        .map(|line| (line.source, line.dash))
        .collect();
    assert_eq!(
        styles,
        [
            (MapSource::Trips, Kind::Recorded.dash()),
            (MapSource::Trips, Kind::Planned.dash()),
            (MapSource::CollectionTracks, Kind::Collections.dash()),
        ]
    );
}

#[test]
fn us90_a_trips_track_is_not_a_collection_tracks_with_the_same_id() {
    use crate::kinds::Kind;
    let items = [of_kind(Kind::Recorded, 7), of_kind(Kind::Collections, 7)];
    let track = TripTrack {
        id: 7,
        coordinates: vec![[9.0, 46.0], [9.1, 46.1]],
    };

    let drawn = lines(
        &items,
        &[(MapSource::CollectionTracks, &track)],
        &heat::marks(&items),
    );

    assert_eq!(drawn.lines.len(), 1);
    assert_eq!(drawn.lines[0].source, MapSource::CollectionTracks);
    assert_eq!(
        activities(&items, &drawn),
        [ActivityType::Hiking],
        "one line, one activity"
    );
}
