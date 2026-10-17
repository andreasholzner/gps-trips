//! The tag summary's list (US-78): everything under the chosen tags, listed
//! per tag at the foot of the screen — trips, collections and collection
//! tracks (US-107) — each with its kind's line in its line's color on the
//! map, so a row and a line can be matched without touching either. What is
//! listed, and in which order, is [`entries`](super::entries)'.

use dioxus::prelude::*;
use trip_archive_types::TagSummaries;

use super::entries::{self, Entry, Row, Uncounted};
use super::Viewer;
use crate::activity_color::Swatch;
use crate::activity_icon::ActivityIcon;
use crate::collection::collection_route;
use crate::format;
use crate::kinds::Kind;
use crate::list_table::KindSwatch;
use crate::map_items::MapSource;
use crate::Route;

/// The lists, under a heading per tag when there are several.
#[component]
pub fn TripsByTag(summary: TagSummaries, viewer: Viewer) -> Element {
    let groups = entries::groups(&summary);
    let headed = groups.len() > 1;
    rsx! {
        section { id: "summary-trips", class: "stats-section",
            h2 { "Trips" }
            for group in groups {
                div { key: "{group.name}", class: "summary-trip-group",
                    if headed {
                        h3 {
                            if let Some(color) = group.color {
                                Swatch { color }
                                " "
                            }
                            "{group.name}"
                        }
                    }
                    TripRows { rows: group.rows, viewer: viewer.clone() }
                }
            }
        }
    }
}

#[component]
fn TripRows(rows: Vec<Row>, viewer: Viewer) -> Element {
    rsx! {
        div { class: "table-scroll",
            table { class: "summary-trips",
                thead {
                    tr {
                        th { scope: "col", "Name" }
                        th { scope: "col", "Date" }
                        th { scope: "col", "Activity" }
                        th { scope: "col", class: "num", "Distance" }
                        th { scope: "col",
                            span { class: "visually-hidden", "Counted" }
                        }
                    }
                }
                tbody {
                    for row in rows {
                        match row {
                            Row::Entry(entry) => rsx! {
                                EntryRow { key: "{entry.source:?}-{entry.id}", entry, viewer: viewer.clone() }
                            },
                            Row::Collection { id, name, color, tracks } => rsx! {
                                tr { key: "collection-{id}", class: "summary-collection",
                                    td { class: "summary-trip-name",
                                        KindSwatch { kind: Kind::Collections, color, name: "Collection" }
                                        Link { to: collection_route(id), "{name}" }
                                    }
                                    td {}
                                    td {}
                                    td {}
                                    td { NotCounted { why: Uncounted::FromCollection } }
                                }
                                for entry in tracks {
                                    EntryRow {
                                        key: "in-{id}-{entry.id}",
                                        entry,
                                        viewer: viewer.clone(),
                                        in_collection: true,
                                    }
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// A trip's or a collection track's row; set in under its collection's when
/// `in_collection`.
#[component]
fn EntryRow(entry: Entry, viewer: Viewer, #[props(default)] in_collection: bool) -> Element {
    let to = match entry.source {
        MapSource::Trips => viewer.trip_route(entry.id),
        MapSource::CollectionTracks => Route::CollectionTrack { id: entry.id },
    };
    rsx! {
        tr { class: if in_collection { "summary-collection-track" },
            td { class: "summary-trip-name",
                KindSwatch { kind: entry.kind, color: entry.color, name: entry.kind_name() }
                Link { to, "{entry.name}" }
            }
            td { {entry.date.as_deref().map_or_else(|| format::or_dash(None), format::day)} }
            td { ActivityIcon { activity: entry.activity } }
            td { class: "num", {format::km(entry.distance_m)} }
            td {
                if let Some(why) = entry.uncounted {
                    NotCounted { why }
                }
            }
        }
    }
}

/// The note on a row the figures leave out, saying why on hover and to a
/// screen reader.
#[component]
fn NotCounted(why: Uncounted) -> Element {
    let reason = why.reason();
    rsx! {
        span { class: "summary-not-counted muted", title: "Not counted: {reason}",
            "not counted"
            span { class: "visually-hidden", ": {reason}" }
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
