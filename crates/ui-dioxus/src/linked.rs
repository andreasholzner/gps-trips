//! What a trip, a collection track or a waypoint is linked to (US-99), on
//! its page: each other end leading to its own page, recorded trips with
//! their dates, and a control removing the link at once — it can be linked
//! again. A share's recipient sees the shared trips a shared trip is linked
//! to, without the control.

use dioxus::prelude::*;
use trip_archive_types::{Links, RecordedTripFilter, SharedLinks};

use crate::api::{self, ApiClient, LinkEnd};
use crate::waypoint::UNNAMED;
use crate::waypoint_icon::WaypointSymbol;
use crate::Route;

/// One kind of other end, as its heading names it.
#[derive(Clone, PartialEq)]
pub struct Group {
    pub title: &'static str,
    pub rows: Vec<Row>,
}

/// One other end.
#[derive(Clone, PartialEq)]
pub struct Row {
    pub name: String,
    /// The local date a recorded trip started on; the collection a track
    /// belongs to.
    pub detail: Option<String>,
    pub to: Route,
    /// A waypoint's symbol, for its icon; `None` for anything else.
    pub symbol: Option<Option<String>>,
    /// The trip whose link removing it names, and the link's other end.
    pub unlink: Option<(i64, LinkEnd)>,
}

/// The groups `end`'s page lists, in the order it lists them, leaving out
/// the empty ones.
pub fn groups(end: LinkEnd, links: &Links) -> Vec<Group> {
    // From a trip's page, the trip names every link; from anything else's,
    // the trip listed does — and only trips are listed there.
    let own_trip = match end {
        LinkEnd::Trip(own) => Some(own),
        LinkEnd::CollectionTrack(_) | LinkEnd::Waypoint(_) => None,
    };
    let trips = |trips: &[trip_archive_types::LinkedTrip]| -> Vec<Row> {
        trips
            .iter()
            .map(|trip| Row {
                name: trip.name.clone(),
                detail: trip.start_date.clone(),
                to: Route::TripDetail { id: trip.id },
                symbol: None,
                unlink: Some(match own_trip {
                    Some(own) => (own, LinkEnd::Trip(trip.id)),
                    None => (trip.id, end),
                }),
            })
            .collect()
    };
    let tracks = links
        .collection_tracks
        .iter()
        .map(|track| Row {
            name: track.name.clone(),
            detail: Some(track.collection_name.clone()),
            to: Route::CollectionTrack { id: track.id },
            symbol: None,
            unlink: own_trip.map(|own| (own, LinkEnd::CollectionTrack(track.id))),
        })
        .collect();
    let waypoints = links
        .waypoints
        .iter()
        .map(|waypoint| Row {
            name: waypoint.name.clone().unwrap_or_else(|| UNNAMED.to_string()),
            detail: Some(waypoint.collection_name.clone()),
            to: Route::Waypoint { id: waypoint.id },
            symbol: Some(waypoint.symbol.clone()),
            unlink: own_trip.map(|own| (own, LinkEnd::Waypoint(waypoint.id))),
        })
        .collect();
    [
        Group {
            title: "Recorded trips",
            rows: trips(&links.recorded_trips),
        },
        Group {
            title: "Planned trips",
            rows: trips(&links.planned_trips),
        },
        Group {
            title: "Collection tracks",
            rows: tracks,
        },
        Group {
            title: "Waypoints",
            rows: waypoints,
        },
    ]
    .into_iter()
    .filter(|group| !group.rows.is_empty())
    .collect()
}

/// The groups a share's recipient reads of a shared trip, leading to the
/// share's own pages.
pub fn shared_groups(token: &str, links: &SharedLinks) -> Vec<Group> {
    let trips = |trips: &[trip_archive_types::SharedLinkedTrip]| -> Vec<Row> {
        trips
            .iter()
            .map(|trip| Row {
                name: trip.name.clone(),
                detail: trip.start_date.clone(),
                to: Route::SharedTripDetail {
                    token: token.to_string(),
                    id: trip.id,
                },
                symbol: None,
                unlink: None,
            })
            .collect()
    };
    [
        Group {
            title: "Recorded trips",
            rows: trips(&links.recorded_trips),
        },
        Group {
            title: "Planned trips",
            rows: trips(&links.planned_trips),
        },
    ]
    .into_iter()
    .filter(|group| !group.rows.is_empty())
    .collect()
}

/// What marks a planned trip, a collection track or a waypoint a recorded
/// trip is linked to — however little of it the trip covered — in a list.
pub const RECORDED_TRIP_MARK: &str = "Has a recorded trip";

/// The mark itself: a glyph, named on hover and to a screen reader.
#[component]
pub fn RecordedTripMark() -> Element {
    rsx! {
        span { class: "recorded-mark", title: RECORDED_TRIP_MARK,
            span { aria_hidden: "true", " ✓" }
            span { class: "visually-hidden", " ({RECORDED_TRIP_MARK})" }
        }
    }
}

/// The choice between everything, what has a recorded trip and what has
/// none — the trip list's filter and a collection page's.
#[component]
pub fn RecordedTripChoice(
    id: &'static str,
    value: Option<RecordedTripFilter>,
    onchange: EventHandler<Option<RecordedTripFilter>>,
) -> Element {
    rsx! {
        label {
            "Recorded trip "
            select {
                id,
                onchange: move |event| onchange.call(event.value().parse().ok()),
                option { value: "", selected: value.is_none(), "— any —" }
                for (filter, label) in [
                    (RecordedTripFilter::With, "With one"),
                    (RecordedTripFilter::Without, "Without one"),
                ] {
                    option {
                        key: "{filter.as_str()}",
                        value: filter.as_str(),
                        selected: value == Some(filter),
                        "{label}"
                    }
                }
            }
        }
    }
}

/// The message of a failed removal, if it failed on `end`'s page.
pub fn error_on(end: LinkEnd, error: &Option<(LinkEnd, String)>) -> Option<&str> {
    match error {
        Some((failed_on, message)) if *failed_on == end => Some(message),
        _ => None,
    }
}

/// The owner's section: what `end` is linked to. Nothing while it is read,
/// nothing for no links, and nothing but a log line if it cannot be read —
/// the page is about something else.
#[component]
pub fn LinkedTo(end: LinkEnd) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    // The end travels back with the answer: the router shows the next page
    // of the same kind through this same scope.
    let mut links = use_resource(use_reactive!(|end| async move {
        (end, api::list_links(&archive(), end).await)
    }));
    // A failed removal, with the page it failed on: the router shows the
    // next page of the same kind through this same scope.
    let mut error = use_signal(|| None::<(LinkEnd, String)>);
    let read = links.read_unchecked();
    let groups = match &*read {
        Some((read_for, Ok(links))) if *read_for == end => groups(end, links),
        Some((read_for, Err(err))) if *read_for == end => {
            dioxus::logger::tracing::error!("could not list the links: {err}");
            Vec::new()
        }
        _ => Vec::new(),
    };
    let on_unlink = move |(trip, other): (i64, LinkEnd)| async move {
        match api::unlink(&archive(), trip, other).await {
            Ok(()) => {
                error.set(None);
                links.restart();
            }
            Err(err) => error.set(Some((end, format!("Could not remove the link: {err}")))),
        }
    };
    rsx! {
        LinkGroups { groups, on_unlink }
        if let Some(message) = error_on(end, &error.read()) {
            p { class: "error", "{message}" }
        }
    }
}

/// A share recipient's section: the shared trips a shared trip is linked
/// to, read on the share's own client.
#[component]
pub fn SharedLinkedTo(token: String, id: i64) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let links = use_resource(use_reactive!(|id| async move {
        (id, api::list_shared_trip_links(&archive(), id).await)
    }));
    let groups = match &*links.read_unchecked() {
        Some((read_for, Ok(links))) if *read_for == id => shared_groups(&token, links),
        _ => Vec::new(),
    };
    rsx! {
        LinkGroups { groups, on_unlink: |_| {} }
    }
}

#[component]
fn LinkGroups(groups: Vec<Group>, on_unlink: EventHandler<(i64, LinkEnd)>) -> Element {
    if groups.is_empty() {
        return rsx! {};
    }
    rsx! {
        section { id: "links",
            h2 { "Linked" }
            for group in groups {
                h3 { key: "{group.title}", "{group.title}" }
                ul { class: "links",
                    for row in group.rows {
                        li {
                            Link { to: row.to.clone(),
                                if let Some(symbol) = row.symbol.clone() {
                                    WaypointSymbol { symbol }
                                }
                                "{row.name}"
                            }
                            if let Some(detail) = &row.detail {
                                span { class: "muted", " · {detail}" }
                            }
                            if let Some(unlink) = row.unlink {
                                " "
                                button {
                                    r#type: "button",
                                    class: "unlink",
                                    title: "Remove the link to {row.name}",
                                    aria_label: "Remove the link to {row.name}",
                                    onclick: move |_| on_unlink.call(unlink),
                                    "×"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
