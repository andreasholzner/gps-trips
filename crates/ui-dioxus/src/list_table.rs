//! The trip list's one table (US-90): the matching trips, newest first, then
//! the matching collections by name, each a group row its tracks fold out
//! of. Every row is selectable, and a selection may mix them (see
//! [`crate::picked`]).

use std::collections::BTreeSet;
use std::ops::Range;

use dioxus::prelude::*;
use trip_archive_types::{
    ActivityType, CollectionSummary, CollectionTrackMark, TripKind, TripSummary,
};

use crate::activity_color;
use crate::activity_icon::ActivityIcon;
use crate::collection::collection_route;
use crate::format;
use crate::kinds::Kind;
use crate::linked::RecordedTripMark;
use crate::pager;
use crate::picked::Picked;
use crate::Route;

/// Which trips and which collections `page` shows. Paging counts trips and
/// collections alike, never the tracks inside a group (US-90), and the
/// trips come first.
pub fn page_rows(page: usize, trips: usize, collections: usize) -> (Range<usize>, Range<usize>) {
    let rows = pager::page_range(page, trips + collections);
    let trip_rows = rows.start.min(trips)..rows.end.min(trips);
    let collection_rows = rows.start.max(trips) - trips..rows.end.max(trips) - trips;
    (trip_rows, collection_rows)
}

/// What a collection's group row says of what it holds: how many of its
/// tracks match out of how many — just how many, when they all do — and
/// its waypoints.
pub fn group_counts(collection: &CollectionSummary) -> String {
    let total = collection.track_count;
    let shown = collection.tracks.len() as i64;
    let noun = if total == 1 { "track" } else { "tracks" };
    let tracks = if shown == total {
        format!("{total} {noun}")
    } else {
        format!("{shown} of {total} {noun}")
    };
    match collection.waypoint_count {
        0 => tracks,
        1 => format!("{tracks} · 1 waypoint"),
        n => format!("{tracks} · {n} waypoints"),
    }
}

/// The table. `trips` and `collections` are the page's; `expanded` names
/// the collections whose tracks are folded out — none to begin with.
#[component]
pub fn ListTable(
    trips: Vec<TripSummary>,
    collections: Vec<CollectionSummary>,
    picked: Signal<BTreeSet<Picked>>,
    expanded: Signal<BTreeSet<i64>>,
) -> Element {
    // Select-all reflects and acts on the rows this page lists, not the
    // tracks inside its groups.
    let listed: Vec<Picked> = trips
        .iter()
        .map(trip_pick)
        .chain(collections.iter().map(|c| Picked::Collection(c.id)))
        .collect();

    rsx! {
        // Eight columns don't fit a phone: the table scrolls inside this
        // box so the page itself never scrolls sideways (US-41), at the
        // width its columns need rather than squeezed to the box's (US-63).
        div { class: "table-scroll",
        table { class: "list-table",
            thead {
                tr {
                    th { PickAll { listed, picked } }
                    th { span { class: "visually-hidden", "Kind" } }
                    th { "Name" }
                    th { "Activity" }
                    th { "Date" }
                    th { "Distance" }
                    th { "Ascent" }
                    th { "Duration" }
                }
            }
            tbody {
                for trip in trips {
                    TripRow { key: "t{trip.id}", trip, picked }
                }
                for collection in collections {
                    CollectionRows { key: "c{collection.id}", collection, picked, expanded }
                }
            }
        }
        }
    }
}

fn trip_pick(trip: &TripSummary) -> Picked {
    Picked::Trip(Kind::of_trip(trip.trip_kind), trip.id)
}

#[component]
fn TripRow(trip: TripSummary, picked: Signal<BTreeSet<Picked>>) -> Element {
    let name = match trip.trip_kind {
        TripKind::Recorded => "Recorded trip",
        TripKind::Planned => "Planned trip",
    };
    rsx! {
        tr {
            td { PickOne { pick: trip_pick(&trip), picked } }
            td { KindSwatch { kind: Kind::of_trip(trip.trip_kind), color: activity_color::color(trip.activity_type), name } }
            td {
                // A router `Link`, so the detail screen (US-42) is reached
                // without a page load — and so the href picks up the
                // bundle's base path, which a hand-written one would have
                // to know about.
                Link { to: Route::TripDetail { id: trip.id }, "{trip.name}" }
                if trip.has_recorded_trip {
                    RecordedTripMark {}
                }
            }
            td { ActivityIcon { activity: trip.activity_type } }
            td { class: "num", {format::or_dash(trip.start_date.as_deref())} }
            td { class: "num", {format::km(trip.distance_m)} }
            td { class: "num", {format::metres(trip.ascent_m)} }
            td { class: "num", {format::duration(trip.duration_secs)} }
        }
    }
}

/// A collection's group row, and its tracks under it once folded out.
#[component]
fn CollectionRows(
    collection: CollectionSummary,
    picked: Signal<BTreeSet<Picked>>,
    mut expanded: Signal<BTreeSet<i64>>,
) -> Element {
    let id = collection.id;
    let open = expanded.read().contains(&id);
    let has_tracks = !collection.tracks.is_empty();
    let activity = collection.activity_type.unwrap_or(ActivityType::Unknown);
    rsx! {
        tr { class: "collection-group",
            td { PickOne { pick: Picked::Collection(id), picked } }
            td { KindSwatch { kind: Kind::Collections, color: activity_color::color(activity), name: "Collection" } }
            td {
                button {
                    r#type: "button",
                    class: "fold",
                    aria_expanded: "{open}",
                    aria_label: if open { "Hide its tracks" } else { "Show its tracks" },
                    disabled: !has_tracks,
                    onclick: move |_| {
                        let mut expanded = expanded.write();
                        if !expanded.remove(&id) {
                            expanded.insert(id);
                        }
                    },
                    if open { "▾" } else { "▸" }
                }
                Link { to: collection_route(id), "{collection.name}" }
            }
            td {
                if let Some(activity) = collection.activity_type {
                    ActivityIcon { activity }
                }
            }
            td { colspan: "5", class: "group-counts", {group_counts(&collection)} }
        }
        if open {
            for track in collection.tracks {
                TrackRow { key: "k{track.id}", track, picked }
            }
        }
    }
}

#[component]
fn TrackRow(track: CollectionTrackMark, picked: Signal<BTreeSet<Picked>>) -> Element {
    rsx! {
        tr { class: "collection-track",
            td { PickOne { pick: Picked::Track(track.id), picked } }
            td { KindSwatch { kind: Kind::Collections, color: activity_color::color(track.activity_type), name: "Collection track" } }
            td {
                Link { to: Route::CollectionTrack { id: track.id }, "{track.name}" }
                if track.has_recorded_trip {
                    RecordedTripMark {}
                }
            }
            td { ActivityIcon { activity: track.activity_type } }
            // A guide book's timestamps are not when anyone went (US-86).
            td { class: "num", {format::or_dash(None)} }
            td { class: "num", {format::km(track.distance_m)} }
            td { class: "num", {format::metres(Some(track.ascent_m))} }
            td { class: "num", {format::duration(None)} }
            td {}
        }
    }
}

/// A short stroke in the line style `kind` is drawn in on the map (US-90),
/// in its line's `color`, or the text's for `None`; `name` says what it
/// stands for on hover and to a screen reader.
#[component]
pub fn KindSwatch(kind: Kind, color: Option<&'static str>, name: &'static str) -> Element {
    rsx! {
        span { class: "kind-swatch", role: "img", title: name, aria_label: name,
            LineSwatch { kind, color }
        }
    }
}

/// The stroke itself — in `color`, or the text's color for `None`. SVG
/// attributes rather than a `style`: the page's CSP refuses inline styles.
#[component]
pub fn LineSwatch(kind: Kind, color: Option<&'static str>) -> Element {
    let color = color.unwrap_or("currentColor");
    rsx! {
        svg {
            class: "swatch",
            view_box: "0 0 24 6",
            "aria-hidden": "true",
            line {
                x1: "3",
                y1: "3",
                x2: "21",
                y2: "3",
                stroke: "{color}",
                stroke_width: "3",
                stroke_linecap: "round",
                stroke_dasharray: kind.dash().unwrap_or("none"),
            }
        }
    }
}

/// Selects every row listed, or none of them.
#[component]
fn PickAll(listed: Vec<Picked>, picked: Signal<BTreeSet<Picked>>) -> Element {
    let all = !listed.is_empty() && listed.iter().all(|p| picked.read().contains(p));
    rsx! {
        input {
            r#type: "checkbox",
            "aria-label": "Select all",
            checked: all,
            onchange: move |event: FormEvent| {
                let mut picked = picked.write();
                for p in &listed {
                    if event.checked() {
                        picked.insert(*p);
                    } else {
                        picked.remove(p);
                    }
                }
            },
        }
    }
}

/// One row's checkbox.
#[component]
fn PickOne(pick: Picked, picked: Signal<BTreeSet<Picked>>) -> Element {
    rsx! {
        input {
            r#type: "checkbox",
            "aria-label": "Select",
            checked: picked.read().contains(&pick),
            onchange: move |event: FormEvent| {
                if event.checked() {
                    picked.write().insert(pick);
                } else {
                    picked.write().remove(&pick);
                }
            },
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
