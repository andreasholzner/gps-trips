//! The Collections tab's table (US-87): one row per collection,
//! linking to its page (US-86), with the row selection a share acts on
//! (US-94).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::CollectionSummary;

use crate::activity_icon::ActivityIcon;
use crate::collection::collection_route;
use crate::selection::{SelectAll, SelectOne};

/// What the Collections tab (US-87) and screen (US-97) say with no
/// collection to list: that there are none, and where one comes from.
pub const NO_COLLECTIONS: &str = "No collections yet. Import a GPX file as a collection.";

/// One row per collection: its name, its activity icon — none for a collection
/// with no activity focus — how many tracks and waypoints it holds, and how
/// many of its tracks have a recorded trip (US-99).
#[component]
pub fn CollectionTable(
    collections: Vec<CollectionSummary>,
    selected: Signal<BTreeSet<i64>>,
) -> Element {
    let listed: Vec<i64> = collections.iter().map(|collection| collection.id).collect();
    rsx! {
        div { class: "table-scroll",
        table {
            thead {
                tr {
                    th { SelectAll { listed, selected } }
                    th { "Collection" }
                    th { "Activity" }
                    th { "Tracks" }
                    th { "With a recorded trip" }
                    th { "Waypoints" }
                }
            }
            tbody {
                for collection in collections {
                    tr { key: "{collection.id}",
                        td { SelectOne { id: collection.id, selected } }
                        td {
                            Link { to: collection_route(collection.id), "{collection.name}" }
                        }
                        td {
                            if let Some(activity) = collection.activity_type {
                                ActivityIcon { activity }
                            }
                        }
                        td { class: "num", "{collection.track_count}" }
                        td { class: "num",
                            {with_recorded_trip(collection.tracks_with_recorded_trip, collection.track_count)}
                        }
                        td { class: "num", "{collection.waypoint_count}" }
                    }
                }
            }
        }
        }
    }
}

/// How many of a collection's `tracks` have a recorded trip, as "12 of 125
/// tracks" (US-99).
pub fn with_recorded_trip(with: i64, tracks: i64) -> String {
    let noun = if tracks == 1 { "track" } else { "tracks" };
    format!("{with} of {tracks} {noun}")
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;
    use trip_archive_types::{ActivityType, CollectionTrackMark};

    fn a_track(id: i64) -> CollectionTrackMark {
        CollectionTrackMark {
            tags: Vec::new(),
            id,
            name: format!("Stage {id}"),
            activity_type: ActivityType::Cycling,
            distance_m: 40_000.0,
            ascent_m: 800.0,
            min_lat: 46.0,
            min_lon: 9.0,
            max_lat: 46.5,
            max_lon: 9.5,
            has_recorded_trip: false,
        }
    }

    fn a_collection(id: i64, name: &str, activity_type: Option<ActivityType>) -> CollectionSummary {
        CollectionSummary {
            tags: Vec::new(),
            id,
            name: name.to_string(),
            activity_type,
            waypoint_count: 46,
            // Narrowed by the filters to fewer than it holds (US-90): the
            // count is of all of them.
            track_count: 125,
            tracks: (1..=7).map(a_track).collect(),
            waypoint_bounds: None,
            tracks_with_recorded_trip: 0,
        }
    }

    fn row<'a>(html: &'a str, name: &str) -> &'a str {
        html.split("<tr")
            .find(|row| row.contains(name))
            .unwrap_or_else(|| panic!("no row for {name}: {html}"))
    }

    #[test]
    fn us87_each_collection_links_to_its_page_with_its_counts() {
        let html = render(|| {
            let collections = vec![a_collection(
                3,
                "Alpencross light",
                Some(ActivityType::Cycling),
            )];
            let selected = use_signal(BTreeSet::new);
            rsx! { CollectionTable { collections, selected } }
        });

        let row = row(&html, "Alpencross light");
        assert!(row.contains("/collections/3"), "{row}");
        assert!(row.contains(">125<"), "the tracks: {row}");
        assert!(row.contains(">46<"), "the waypoints: {row}");
    }

    #[test]
    fn us99_each_collection_counts_its_tracks_with_a_recorded_trip() {
        let html = render(|| {
            let collections = vec![CollectionSummary {
                tracks_with_recorded_trip: 12,
                ..a_collection(3, "Alpencross light", Some(ActivityType::Cycling))
            }];
            let selected = use_signal(BTreeSet::new);
            rsx! { CollectionTable { collections, selected } }
        });

        assert!(
            row(&html, "Alpencross light").contains(">12 of 125 tracks<"),
            "{html}"
        );
        assert_eq!(with_recorded_trip(0, 1), "0 of 1 track");
    }

    #[test]
    fn us87_a_collections_activity_shows_as_its_icon_and_none_without_one() {
        let html = render(|| {
            let collections = vec![
                a_collection(1, "Bike book", Some(ActivityType::Cycling)),
                a_collection(2, "Mixed book", None),
            ];
            let selected = use_signal(BTreeSet::new);
            rsx! { CollectionTable { collections, selected } }
        });

        let with = row(&html, "Bike book");
        assert!(with.contains(r#"class="activity-icon""#), "{with}");
        assert!(with.contains(r#"aria-label="Cycling""#), "{with}");
        let without = row(&html, "Mixed book");
        assert!(!without.contains("activity-icon"), "{without}");
    }

    #[test]
    fn us94_each_collection_is_selectable_and_so_are_all_at_once() {
        let html = render(|| {
            let collections = vec![
                a_collection(1, "Bike book", None),
                a_collection(2, "Mixed book", None),
            ];
            let selected = use_signal(|| BTreeSet::from([2]));
            rsx! { CollectionTable { collections, selected } }
        });

        assert_eq!(
            html.matches(r#"type="checkbox""#).count(),
            3,
            "one per row plus select-all: {html}"
        );
        assert!(!row(&html, "Bike book").contains("checked"), "{html}");
        assert!(row(&html, "Mixed book").contains("checked"), "{html}");
    }
}
