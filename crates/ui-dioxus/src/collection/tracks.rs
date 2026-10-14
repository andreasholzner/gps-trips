//! A collection's tracks in file order, each leading to its own page and
//! marked when it has a recorded trip (US-99); the owner selects some of
//! them here to share them (US-94).

use std::collections::BTreeSet;

use dioxus::prelude::*;
use trip_archive_types::{ActivityType, CollectionTrackSummary};

use crate::activity_icon::ActivityIcon;
use crate::format;
use crate::linked::RecordedTripMark;
use crate::selection::{SelectAll, SelectOne};
use crate::share::{Selected, ShareSelectedPanel};
use crate::Route;

use super::CollectionView;

/// The owner's list: the tracks `view` keeps, selectable, and sharing the
/// selection.
#[component]
pub(super) fn Tracks(
    id: i64,
    tracks: Vec<CollectionTrackSummary>,
    view: CollectionView,
) -> Element {
    let mut selected = use_signal(BTreeSet::new);
    // The router shows the next collection through this same scope; a
    // selection of this one's tracks must not carry over to it.
    use_effect(use_reactive!(|id| {
        let _ = id;
        selected.write().clear();
    }));
    let held = !tracks.is_empty();
    let tracks: Vec<CollectionTrackSummary> = tracks
        .into_iter()
        .filter(|track| view.keeps(track.has_recorded_trip))
        .collect();
    // A track the filter hides leaves the selection, as a kind switched off
    // does on the trip list (US-90): nothing out of sight is shared.
    use_effect(use_reactive!(|tracks| {
        let listed = listed_selection(&selected.peek(), &tracks);
        if listed.len() != selected.peek().len() {
            selected.set(listed.into_iter().collect());
        }
    }));
    let ids = listed_selection(&selected.read(), &tracks);
    rsx! {
        div { class: "bulk-panels",
            ShareSelectedPanel { ids, of: Selected::CollectionTracks }
        }
        if held && tracks.is_empty() {
            p { id: "no-matching-track", "No track matches." }
        } else {
            TrackList { tracks, selected }
        }
    }
}

/// The selected tracks among those `listed`, in their order.
pub(super) fn listed_selection(
    selected: &BTreeSet<i64>,
    listed: &[CollectionTrackSummary],
) -> Vec<i64> {
    listed
        .iter()
        .map(|track| track.id)
        .filter(|id| selected.contains(id))
        .collect()
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
                                if track.has_recorded_trip {
                                    RecordedTripMark {}
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
