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
    // Re-runs whenever the query string changes — reading `filters` inside
    // the closure is the whole subscription; there is no dependency list to
    // keep in sync by hand.
    let trips =
        use_resource(move || async move { api::list_trips(filters.read().to_query()).await });

    rsx! {
        h1 { "Trips" }
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
