//! US-99 — a collection's tracks and waypoints with a recorded trip:
//! counted, marked, and the lists narrowed to them or to the rest.

use super::*;
use crate::test_support::import_sample;
use trip_archive_types::{CreateLinks, RecordedTripFilter};

fn narrowed(recorded_trip: RecordedTripFilter) -> CollectionView {
    CollectionView {
        recorded_trip: Some(recorded_trip),
    }
}

#[test]
fn us99_the_page_counts_its_tracks_with_a_recorded_trip() {
    let mut collection = a_collection();
    collection.tracks.push(CollectionTrackSummary {
        has_recorded_trip: true,
        ..a_track(2, "Stage 2", ActivityType::Cycling)
    });

    assert_eq!(
        with_recorded_trip(&collection),
        "1 of 2 tracks with a recorded trip"
    );
}

#[test]
fn us99_a_waypoint_with_a_recorded_trip_is_marked() {
    let html = render(|| {
        rsx! {
            WaypointList {
                waypoints: vec![a_waypoint(1, Some("Pass"), None), a_waypoint(2, Some("Hut"), None)],
                with_recorded_trip: vec![1],
            }
        }
    });

    let row = |name: &str| {
        html.split("<tr")
            .find(|row| row.contains(name))
            .unwrap_or_else(|| panic!("no {name}: {html}"))
            .to_string()
    };
    assert!(row("Pass").contains("Has a recorded trip"), "{html}");
    assert!(!row("Hut").contains("Has a recorded trip"), "{html}");
}

// Against a real archive (ADR-0012): one stage linked to a recorded trip,
// the page narrowed by its URL either way.
#[tokio::test]
async fn us99_the_lists_are_narrowed_to_the_tracks_with_or_without_a_recorded_trip() {
    let (archive, _dir) = serve_test_archive().await;
    let (id, tracks) = import_collection(&archive, COLLECTION_GPX, &[("name", "Lakes")]).await;
    let monday = import_sample(&archive, &[]).await;
    api::create_links(
        &archive,
        &CreateLinks {
            trip_ids: vec![monday],
            collection_track_ids: vec![tracks[0]],
        },
    )
    .await
    .unwrap();
    let href = |track: i64| format!(r#"href="/collection-tracks/{track}""#);

    let with = render_against_archive(
        &archive,
        move || rsx! { Collection { id, view: narrowed(RecordedTripFilter::With) } },
        |html| html.contains("collection-tracks"),
    )
    .await;
    let without = render_against_archive(
        &archive,
        move || rsx! { Collection { id, view: narrowed(RecordedTripFilter::Without) } },
        |html| html.contains("collection-tracks"),
    )
    .await;

    assert!(
        with.contains("1 of 3 tracks with a recorded trip"),
        "{with}"
    );
    assert!(with.contains(&href(tracks[0])), "{with}");
    assert!(with.contains("Has a recorded trip"), "{with}");
    for track in &tracks[1..] {
        assert!(!with.contains(&href(*track)), "{with}");
        assert!(without.contains(&href(*track)), "{without}");
    }
    assert!(!without.contains(&href(tracks[0])), "{without}");
    assert!(
        with.contains(r#"<option value="with" selected=true>"#),
        "the choice comes from the URL: {with}"
    );
}

#[test]
fn us99_a_filter_keeping_no_track_says_so() {
    let html = render(|| {
        rsx! {
            Tracks {
                id: 2,
                tracks: vec![a_track(1, "Stage 1", ActivityType::Cycling)],
                view: narrowed(RecordedTripFilter::With),
            }
        }
    });

    assert!(html.contains("No track matches."), "{html}");
    assert!(!html.contains("waypoints only"), "{html}");
}
