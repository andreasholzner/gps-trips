//! A collection's tracks in file order, each leading to its own page; the
//! owner selects some of them here to share them (US-94).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, CollectionTrackSummary};

use crate::activity_icon::ActivityIcon;
use crate::format;
use crate::selection::{SelectAll, SelectOne};
use crate::share::{Selected, ShareSelectedPanel};
use crate::Route;

/// The owner's list: the tracks, selectable, and sharing the selection.
#[component]
pub(super) fn Tracks(id: i64, tracks: Vec<CollectionTrackSummary>) -> Element {
    let mut selected = use_signal(BTreeSet::new);
    // The router shows the next collection through this same scope; a
    // selection of this one's tracks must not carry over to it.
    use_effect(use_reactive!(|id| {
        let _ = id;
        selected.write().clear();
    }));
    rsx! {
        div { class: "bulk-panels",
            ShareSelectedPanel { selected, of: Selected::CollectionTracks }
        }
        TrackList { tracks, selected }
    }
}

/// Where a track's row or line leads: its page, or a share's page of it.
pub fn track_route(token: Option<&str>, id: i64) -> Route {
    match token {
        None => Route::CollectionTrack { id },
        Some(token) => Route::SharedCollectionTrackPage {
            token: token.to_string(),
            id,
        },
    }
}

/// The tracks in file order, each leading to its own page — the share's,
/// for a share's `token` (US-94) — with a checkbox each when `selected` is
/// given.
#[component]
pub fn TrackList(
    tracks: Vec<CollectionTrackSummary>,
    selected: Option<Signal<BTreeSet<i64>>>,
    token: Option<String>,
) -> Element {
    if tracks.is_empty() {
        return rsx! { p { "This collection holds waypoints only." } };
    }
    let listed: Vec<i64> = tracks.iter().map(|track| track.id).collect();
    rsx! {
        div { class: "table-scroll",
            table { id: "collection-tracks",
                thead {
                    tr {
                        if let Some(selected) = selected {
                            th { SelectAll { listed, selected } }
                        }
                        th { scope: "col", "Track" }
                        th { scope: "col", class: "num", "Distance" }
                        th { scope: "col", class: "num", "Ascent" }
                        th { scope: "col", class: "num", "Descent" }
                    }
                }
                tbody {
                    for track in tracks {
                        tr { key: "{track.id}",
                            if let Some(selected) = selected {
                                td { SelectOne { id: track.id, selected } }
                            }
                            td {
                                Link { to: track_route(token.as_deref(), track.id),
                                    if track.activity_type != ActivityType::Unknown {
                                        ActivityIcon { activity: track.activity_type }
                                    }
                                    "{track.name}"
                                }
                            }
                            td { class: "num", {format::km(track.distance_m)} }
                            td { class: "num", {format::metres(Some(track.ascent_m))} }
                            td { class: "num", {format::metres(Some(track.descent_m))} }
                        }
                    }
                }
            }
        }
    }
}
