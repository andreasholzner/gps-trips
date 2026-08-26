//! The trip list (US-6/US-13/US-32): one tab's worth of trips, narrowed by
//! the filter panel, each row linking to its detail screen.

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, TripKind, TripSummary};

use crate::api;
use crate::filters::Filters;
use crate::format;
use crate::Route;

#[component]
pub fn TripList() -> Element {
    let filters = use_signal(Filters::default);
    let base_url = use_context::<Signal<String>>();
    // Re-runs whenever the query string or the configured archive changes —
    // reading the signals inside the closure is the whole subscription; there
    // is no dependency list to keep in sync by hand.
    let trips = use_resource(move || async move {
        api::list_trips(&base_url(), filters.read().to_query()).await
    });

    rsx! {
        h1 { "Trips" }
        p { Link { to: Route::Settings {}, "Settings" } }
        KindTabs { filters }
        FilterPanel { filters }
        match &*trips.read_unchecked() {
            None => rsx! { p { "Loading…" } },
            Some(Err(err)) => rsx! { p { class: "error", "Could not load trips: {err}" } },
            Some(Ok(trips)) if trips.is_empty() => rsx! { EmptyState { filters } },
            Some(Ok(trips)) => rsx! { TripTable { trips: trips.clone() } },
        }
    }
}

/// The Recorded/Planned tabs (US-32). Switching tabs keeps every other
/// filter, the same way the server-rendered page's tab forms do.
#[component]
fn KindTabs(filters: Signal<Filters>) -> Element {
    rsx! {
        nav {
            for kind in TripKind::ALL {
                button {
                    key: "{kind}",
                    disabled: filters.read().kind == kind,
                    onclick: move |_| filters.write().kind = kind,
                    "{kind.label()}"
                }
            }
        }
    }
}

/// The filter form (US-13). Every input writes straight into the shared
/// `Filters` signal, so the list re-queries as the owner types — no submit
/// button, and no separate "pending vs. applied" copy of the state.
#[component]
fn FilterPanel(filters: Signal<Filters>) -> Element {
    rsx! {
        fieldset {
            legend { "Filter" }
            label {
                "Search "
                input {
                    r#type: "search",
                    value: "{filters.read().q}",
                    oninput: move |event| filters.write().q = event.value(),
                }
            }
            label {
                "Activity "
                select {
                    value: filters.read().activity.map_or("", |activity| activity.as_str()),
                    onchange: move |event| {
                        filters.write().activity = event.value().parse::<ActivityType>().ok();
                    },
                    option { value: "", "— any —" }
                    for activity in ActivityType::SELECTABLE {
                        option { key: "{activity}", value: activity.as_str(), "{activity.label()}" }
                    }
                }
            }
            label {
                "From "
                input {
                    r#type: "date",
                    value: "{filters.read().from}",
                    oninput: move |event| filters.write().from = event.value(),
                }
            }
            label {
                "To "
                input {
                    r#type: "date",
                    value: "{filters.read().to}",
                    oninput: move |event| filters.write().to = event.value(),
                }
            }
            label {
                "Min km "
                input {
                    r#type: "number",
                    min: "0",
                    value: "{filters.read().min_dist}",
                    oninput: move |event| filters.write().min_dist = event.value(),
                }
            }
            label {
                "Max km "
                input {
                    r#type: "number",
                    min: "0",
                    value: "{filters.read().max_dist}",
                    oninput: move |event| filters.write().max_dist = event.value(),
                }
            }
            button {
                onclick: move |_| {
                    // Clearing keeps the tab the owner is on.
                    let kind = filters.read().kind;
                    filters.set(Filters { kind, ..Default::default() });
                },
                "Clear filters"
            }
        }
    }
}

#[component]
fn EmptyState(filters: Signal<Filters>) -> Element {
    let filters = filters.read();
    if filters.any_set() {
        rsx! { p { "No trips match your filters." } }
    } else if filters.kind == TripKind::Planned {
        rsx! { p { "No planned trips yet." } }
    } else {
        rsx! { p { "No trips yet." } }
    }
}

#[component]
fn TripTable(trips: Vec<TripSummary>) -> Element {
    rsx! {
        table {
            thead {
                tr {
                    th { "Trip" }
                    th { "Activity" }
                    th { "Date" }
                    th { "Distance" }
                    th { "Ascent" }
                    th { "Duration" }
                }
            }
            tbody {
                for trip in trips {
                    tr { key: "{trip.id}",
                        td {
                            Link { to: Route::TripDetail { id: trip.id }, "{trip.name}" }
                        }
                        td { "{trip.activity_type.label()}" }
                        td { {format::date(trip.start_time.as_deref())} }
                        td { {format::km(trip.distance_m)} }
                        td { {format::metres(trip.ascent_m)} }
                        td { {format::duration(trip.duration_secs)} }
                    }
                }
            }
        }
    }
}

// ── Exemplar view-layer tests (ADR-0012 amendment) ───────────────────────────
//
// Two levels, deliberately kept as worked examples: one component rendered
// from props, one whole screen against a real server. `test_support` holds the
// harness; see its module docs for what this style of test cannot reach.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{render, render_against_archive, serve_test_archive};
    use trip_archive_types::ActivityType;

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

    #[test]
    fn the_trip_table_shows_a_row_per_trip_linking_to_its_detail() {
        let trips = vec![a_trip(1, "Oslo Hills Walk"), a_trip(2, "Inn Valley Ride")];

        let html = render(move || rsx! { TripTable { trips: trips.clone() } });

        assert!(html.contains("Oslo Hills Walk"), "{html}");
        assert!(html.contains("Inn Valley Ride"), "{html}");
        // The formatting the screen is responsible for, not the raw values.
        assert!(html.contains("12.35 km"), "{html}");
        assert!(html.contains("01:02:05"), "{html}");
        assert!(html.contains("2026-07-11"), "{html}");
        assert!(
            html.contains("/trips/1"),
            "each row links to its trip: {html}"
        );
    }

    #[tokio::test]
    async fn the_list_screen_reports_an_empty_archive() {
        // The whole screen against a real server: the fetch, the filter query,
        // and the empty state the owner actually sees. Nothing is mocked.
        let (base_url, _dir) = serve_test_archive().await;

        let html = render_against_archive(
            &base_url,
            || rsx! { TripList {} },
            |html| !html.contains("Loading"),
        )
        .await;

        assert!(html.contains("No trips yet"), "{html}");
    }
}
