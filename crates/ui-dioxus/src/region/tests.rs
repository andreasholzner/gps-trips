//! The region map's tests (ADR-0012), split out to keep `region.rs` under
//! the repo's file-length cap. Panning, zooming and clicking a line are the
//! browser layer's.

use super::*;
use crate::kinds::{Kind, Kinds};
use crate::map_items::MapItem;
use crate::test_support::render;
use trip_archive_types::{TripKind, TripSummary};

/// A trip near Oslo, so it gets a mark.
fn a_trip(activity_type: ActivityType) -> TripSummary {
    TripSummary {
        id: 1,
        name: "Trip".to_string(),
        activity_type,
        start_time: None,
        start_date: None,
        distance_m: 1_000.0,
        ascent_m: None,
        duration_secs: None,
        trip_kind: TripKind::Recorded,
        privacy_status: None,
        min_lat: Some(59.9),
        min_lon: Some(10.7),
        max_lat: Some(60.0),
        max_lon: Some(10.8),
        has_recorded_trip: false,
    }
}

#[test]
fn the_map_is_in_view_without_opening_anything() {
    // US-63: the map is the middle of the screen, not a filter's fine
    // print behind a disclosure.
    let html = render(|| {
        let filters = Signal::new(Filters::default());
        rsx! { RegionFilter { filters, mapped: None } }
    });

    assert!(html.contains("region-map"), "{html}");
    assert!(!html.contains("<details"), "{html}");
    for control in ["Filter to map", "Fit to items"] {
        assert!(html.contains(control), "{control}: {html}");
    }
}

#[test]
fn us92_the_rectangle_and_its_buttons_are_gone() {
    let html = render(|| {
        let filters = Signal::new(Filters::default());
        rsx! { RegionFilter { filters, mapped: None } }
    });

    for control in ["Select area", "Cancel selection", "Clear region"] {
        assert!(!html.contains(control), "{control}: {html}");
    }
}

#[test]
fn us92_filtering_to_the_map_is_on_exactly_when_the_filters_hold_a_region() {
    // The checkbox has no state of its own: a region in the URL ticks
    // it, and "Clear filters" unticks it.
    let off = render(|| {
        let filters = Signal::new(Filters::default());
        rsx! { RegionFilter { filters, mapped: None } }
    });
    let on = render(|| {
        let filters = Signal::new(Filters {
            bbox: "10.75,59.91,11.25,60.12".to_string(),
            ..Filters::default()
        });
        rsx! { RegionFilter { filters, mapped: None } }
    });

    assert!(off.contains(r#"id="region-follow""#), "{off}");
    assert!(!off.contains("checked"), "{off}");
    assert!(on.contains("checked"), "{on}");
}

#[test]
fn us92_filtering_to_the_map_waits_until_the_map_says_where_it_is() {
    // Before the first view is reported there is no region to take.
    let html = render(|| {
        let filters = Signal::new(Filters::default());
        rsx! { RegionFilter { filters, mapped: None } }
    });

    assert!(html.contains("disabled"), "{html}");
}

fn view(bounds: [f64; 4]) -> Viewport {
    Viewport { zoom: 9.0, bounds }
}

#[test]
fn us92_the_region_is_the_visible_area() {
    assert_eq!(
        region_of(&view([10.5, 59.8, 11.0, 60.0])),
        [10.5, 59.8, 11.0, 60.0]
    );
}

#[test]
fn us92_a_view_across_the_antimeridian_covers_every_longitude() {
    // The API rejects a region that crosses it (ADR-0011).
    assert_eq!(
        region_of(&view([170.0, -20.0, 190.0, -10.0])),
        [-180.0, -20.0, 180.0, -10.0]
    );
}

#[test]
fn us92_a_view_wider_than_the_world_covers_every_longitude() {
    assert_eq!(
        region_of(&view([-180.0, -60.0, 300.0, 80.0])),
        [-180.0, -60.0, 180.0, 80.0]
    );
    assert_eq!(
        region_of(&view([-250.0, -60.0, 100.0, 80.0])),
        [-180.0, -60.0, 180.0, 80.0]
    );
}

#[test]
fn us92_with_filtering_off_the_view_narrows_nothing() {
    assert_eq!(followed("", &view([10.5, 59.8, 11.0, 60.0])), None);
}

#[test]
fn us92_with_filtering_on_the_region_follows_the_view() {
    assert_eq!(
        followed("-30,30,-20,40", &view([10.5, 59.8, 11.0, 60.0])),
        Some("10.500000,59.800000,11.000000,60.000000".to_string())
    );
}

#[test]
fn us92_a_view_that_did_not_move_changes_nothing() {
    // A settle without a move must not re-query the list.
    let bounds = [10.5, 59.8, 11.0, 60.0];
    let held = interop::bbox_param(bounds);

    assert_eq!(followed(&held, &view(bounds)), None);
}

#[test]
fn the_map_names_the_colors_of_the_activities_it_marks() {
    // US-75.
    let html = render(|| {
        let filters = Signal::new(Filters::default());
        let trips = vec![a_trip(ActivityType::Hiking), a_trip(ActivityType::Kayaking)];
        rsx! { RegionFilter { filters, mapped: Some(Mapped::of(&trips, &[])) } }
    });

    assert!(html.contains("map-legend"), "{html}");
    assert!(
        html.contains("Hiking") && html.contains("Kayaking"),
        "{html}"
    );
}

#[test]
fn zoomed_in_the_legend_names_the_activities_of_the_lines_drawn() {
    // US-73: a kayak trip marked elsewhere but with no line in view is
    // not on the map, so not in its legend.
    let trips = Mapped::of(
        &[a_trip(ActivityType::Hiking), a_trip(ActivityType::Kayaking)],
        &[],
    );
    let trips = &trips.items[..];

    assert_eq!(
        legend_activities(Some(vec![ActivityType::Hiking]), Some(trips)),
        Some(vec![ActivityType::Hiking])
    );
    assert_eq!(
        legend_activities(None, Some(trips)),
        Some(vec![ActivityType::Hiking, ActivityType::Kayaking])
    );
    assert_eq!(legend_activities(None, None), None);
}

#[test]
fn us90_the_fit_button_names_no_kind_whatever_is_on() {
    let on = |kinds| {
        render(move || {
            let filters = Signal::new(Filters {
                kinds,
                ..Filters::default()
            });
            rsx! { RegionFilter { filters, mapped: None } }
        })
    };

    assert!(on(Kinds::default()).contains("Fit to items"));
    let collections = on(Kinds::only(Kind::Collections));
    assert!(collections.contains("Fit to items"), "{collections}");
    assert!(
        collections.contains("Filter to map"),
        "the region narrows collections too: {collections}"
    );
}

#[test]
fn us87_a_click_on_a_line_opens_its_trip_or_collection_track() {
    assert!(matches!(
        route_for(MapSource::Trips, 7),
        Route::TripDetail { id: 7 }
    ));
    assert!(matches!(
        route_for(MapSource::CollectionTracks, 7),
        Route::CollectionTrack { id: 7 }
    ));
}

#[test]
fn the_map_is_not_busy_before_it_asks_for_anything() {
    let html = render(|| {
        let filters = Signal::new(Filters::default());
        rsx! { RegionFilter { filters, mapped: None } }
    });

    assert!(!html.contains("region-map-busy"), "{html}");
}

#[test]
fn while_the_tracks_load_the_map_says_so() {
    // US-73: switching to lines waits on the archive, and says so.
    let html = render(|| rsx! { MapBusy {} });

    assert!(html.contains(r#"role="status""#), "{html}");
    assert!(html.contains("Loading tracks…"), "{html}");
}

// US-87: while the tracks in view load, the map keeps what it shows — unless
// that is lines the list no longer holds, after a kind is switched off or the
// list narrowed.
fn mapped(ids: &[i64], tracks: &[i64]) -> Mapped {
    let trips: Vec<TripSummary> = ids
        .iter()
        .map(|&id| TripSummary {
            id,
            ..a_trip(ActivityType::Hiking)
        })
        .collect();
    let mut mapped = Mapped::of(&trips, &[]);
    let template = mapped.items[0].clone();
    mapped.items.extend(tracks.iter().map(|&id| MapItem {
        kind: Kind::Collections,
        id,
        ..template.clone()
    }));
    mapped
}

fn drawn(keys: &[(MapSource, i64)]) -> DrawnLines {
    DrawnLines {
        keys: keys.to_vec(),
    }
}

#[test]
fn us87_lines_of_what_the_list_still_holds_are_kept_while_more_load() {
    let drawn = drawn(&[(MapSource::Trips, 1), (MapSource::CollectionTracks, 2)]);

    assert!(!outlived(Some(&drawn), &mapped(&[1, 3], &[2])));
    assert!(!outlived(None, &mapped(&[1], &[])));
}

#[test]
fn us90_lines_of_a_kind_switched_off_are_taken_off_while_the_rest_load() {
    // Trip 1 and collection track 1 share an id; the track is gone.
    let drawn = drawn(&[(MapSource::Trips, 1), (MapSource::CollectionTracks, 1)]);

    assert!(outlived(Some(&drawn), &mapped(&[1], &[])));
}

#[test]
fn lines_of_trips_no_longer_listed_are_taken_off_while_the_new_ones_load() {
    let drawn = drawn(&[(MapSource::Trips, 1), (MapSource::Trips, 2)]);

    assert!(outlived(Some(&drawn), &mapped(&[2, 3], &[])));
}
