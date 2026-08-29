//! The trip table (US-6): one row per trip, with the row selection the
//! bulk-tag panel acts on (US-34) and the linked Komoot tour's privacy
//! (US-35).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::TripSummary;

use crate::format;

/// One row per trip: name, activity, date, distance, ascent, duration
/// (US-6), and the linked Komoot tour's privacy (US-35). The name becomes a
/// link to the trip's detail screen when US-42 builds it; until then the row
/// is display-only.
#[component]
pub fn TripTable(trips: Vec<TripSummary>, selected: Signal<BTreeSet<i64>>) -> Element {
    let listed: Vec<i64> = trips.iter().map(|trip| trip.id).collect();
    // Select-all reflects and acts on the trips actually listed, so a
    // filtered list can't select a trip the owner cannot see (US-34).
    let all_listed_selected =
        !listed.is_empty() && listed.iter().all(|id| selected.read().contains(id));

    rsx! {
        // Eight columns don't fit a phone: the table scrolls inside this
        // box so the page itself never scrolls sideways (US-41).
        div { class: "table-scroll",
        table {
            thead {
                tr {
                    th {
                        input {
                            r#type: "checkbox",
                            checked: all_listed_selected,
                            onchange: move |event: FormEvent| {
                                let mut selected = selected.write();
                                for id in &listed {
                                    if event.checked() {
                                        selected.insert(*id);
                                    } else {
                                        selected.remove(id);
                                    }
                                }
                            },
                        }
                    }
                    th { "Trip" }
                    th { "Activity" }
                    th { "Date" }
                    th { "Distance" }
                    th { "Ascent" }
                    th { "Duration" }
                    th { "Privacy" }
                }
            }
            tbody {
                for trip in trips {
                    tr { key: "{trip.id}",
                        td {
                            input {
                                r#type: "checkbox",
                                checked: selected.read().contains(&trip.id),
                                onchange: move |event: FormEvent| {
                                    if event.checked() {
                                        selected.write().insert(trip.id);
                                    } else {
                                        selected.write().remove(&trip.id);
                                    }
                                },
                            }
                        }
                        td {
                            // A plain link, not a `Link`: until US-42 builds
                            // the detail screen, this leaves the SPA for the
                            // server-rendered page that still owns editing,
                            // tagging and reliving a trip. It becomes a
                            // client-side route then.
                            a { href: "/trips/{trip.id}", "{trip.name}" }
                        }
                        td { "{trip.activity_type.label()}" }
                        td { {format::date(trip.start_time.as_deref())} }
                        td { {format::km(trip.distance_m)} }
                        td { {format::metres(trip.ascent_m)} }
                        td { {format::duration(trip.duration_secs)} }
                        td { {format::privacy(trip.privacy_status)} }
                    }
                }
            }
        }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;
    use trip_archive_types::{ActivityType, KomootPrivacy, TripKind};

    fn a_trip(id: i64, name: &str) -> TripSummary {
        TripSummary {
            id,
            name: name.to_string(),
            activity_type: ActivityType::Hiking,
            start_time: Some("2026-07-11T09:30:00Z".to_string()),
            distance_m: 12_345.0,
            ascent_m: Some(410.0),
            duration_secs: Some(3_725),
            trip_kind: TripKind::Recorded,
            privacy_status: None,
        }
    }

    // Exemplar for the component-render layer (ADR-0012, 2026-08-26a): props
    // in, HTML string out, no server. US-6: each trip's name, date, distance,
    // ascent and duration — asserting the formatting the screen is
    // responsible for, not the raw values.
    #[test]
    fn the_trip_table_shows_a_row_per_trip() {
        let trips = vec![a_trip(1, "Oslo Hills Walk"), a_trip(2, "Inn Valley Ride")];

        let html = render(move || {
            let selected = Signal::new(BTreeSet::new());
            rsx! { TripTable { trips: trips.clone(), selected } }
        });

        assert!(html.contains("Oslo Hills Walk"), "{html}");
        assert!(html.contains("Inn Valley Ride"), "{html}");
        // Each row reaches its trip — the server-rendered detail page for
        // now, an SPA route once US-42 lands.
        assert!(html.contains("/trips/1"), "{html}");
        assert!(html.contains("2026-07-11"), "{html}");
        assert!(html.contains("12.35 km"), "{html}");
        assert!(html.contains("410 m"), "{html}");
        assert!(html.contains("01:02:05"), "{html}");
        assert!(html.contains("Hiking"), "{html}");
    }

    // US-34: the owner can select rows individually or all at once. The
    // clicking itself is a real event — browser layer.
    #[test]
    fn every_row_is_selectable_and_so_are_all_rows_at_once() {
        let trips = vec![a_trip(1, "Oslo Hills Walk"), a_trip(2, "Inn Valley Ride")];

        let html = render(move || {
            let selected = Signal::new(BTreeSet::from([1_i64]));
            rsx! { TripTable { trips: trips.clone(), selected } }
        });

        assert_eq!(
            html.matches("type=\"checkbox\"").count(),
            3,
            "one per row plus select-all: {html}"
        );
        assert!(
            html.contains("checked"),
            "the selected row is checked: {html}"
        );
    }

    // US-35's Privacy column. A linked trip's privacy is a stored value the
    // row displays, so the component layer covers it (ADR-0012); seeding a
    // real Komoot link would need the mocked client the server's own US-35
    // tests use, and would assert nothing more about this screen.
    #[test]
    fn the_privacy_column_shows_a_linked_trips_privacy() {
        let trips = vec![TripSummary {
            privacy_status: Some(KomootPrivacy::Public),
            ..a_trip(1, "Komoot Trip")
        }];

        let html = render(move || {
            let selected = Signal::new(BTreeSet::new());
            rsx! { TripTable { trips: trips.clone(), selected } }
        });

        assert!(html.contains("Privacy"), "{html}");
        assert!(html.contains(KomootPrivacy::Public.label()), "{html}");
    }

    #[test]
    fn a_privacy_komoot_reported_unmappably_shows_as_unknown() {
        // The archive never pretends to know a privacy it couldn't map
        // (ADR-0021); it shows it, and the push side leaves it alone.
        let trips = vec![TripSummary {
            privacy_status: Some(KomootPrivacy::Unknown),
            ..a_trip(1, "Odd Privacy")
        }];

        let html = render(move || {
            let selected = Signal::new(BTreeSet::new());
            rsx! { TripTable { trips: trips.clone(), selected } }
        });

        assert!(html.contains(KomootPrivacy::Unknown.label()), "{html}");
    }

    #[test]
    fn a_trip_that_never_came_from_komoot_shows_no_privacy() {
        let trips = vec![a_trip(1, "Local Trip")];

        let html = render(move || {
            let selected = Signal::new(BTreeSet::new());
            rsx! { TripTable { trips: trips.clone(), selected } }
        });

        assert!(
            html.contains("Privacy"),
            "the column is always there: {html}"
        );
        assert!(!html.contains(KomootPrivacy::Public.label()), "{html}");
        assert!(!html.contains(KomootPrivacy::Private.label()), "{html}");
    }

    #[test]
    fn a_trip_missing_optional_stats_shows_dashes_not_blanks() {
        let trips = vec![TripSummary {
            start_time: None,
            ascent_m: None,
            duration_secs: None,
            ..a_trip(1, "Bare Trip")
        }];

        let html = render(move || {
            let selected = Signal::new(BTreeSet::new());
            rsx! { TripTable { trips: trips.clone(), selected } }
        });

        assert!(html.contains("—"), "{html}");
    }
}
