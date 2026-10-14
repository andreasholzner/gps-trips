//! Shared collections (US-94), as their recipient sees them: one
//! collection's page — its description, map, tracks and waypoints — or,
//! for several, every track and waypoint on one map above a list of the
//! collections. Never a tag, the files it came from, or a control; a
//! waypoint is listed without a page of its own.

use dioxus::prelude::*;
use trip_archive_types::{
    CollectionTrackSummary, ShareOverview, SharedCollectionTrack, SharedWaypoint, Waypoint,
};

use super::{title, ShareError};
use crate::activity_color::ActivityLegend;
use crate::activity_icon::ActivityIcon;
use crate::api::{self, ApiClient};
use crate::collection::{collection_lines, counts, track_route, TrackList, WaypointList};
use crate::interop::{self, MapWaypoint, OverviewEvent};
use crate::Route;

/// `/s/:token/collections/:id` — one collection of a share of several.
#[component]
pub fn SharedCollectionPage(token: String, id: i64) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let overview = use_resource(move || async move { api::share_overview(&archive()).await });

    match &*overview.read_unchecked() {
        None => rsx! { p { "Loading…" } },
        Some(Err(err)) => rsx! { ShareError { err: err.clone() } },
        Some(Ok(overview)) => rsx! {
            SharedCollectionView { token, id, overview: overview.clone() }
        },
    }
}

/// The recipient's track as the owner's lists take it. Built here, on the
/// recipient's side, from what the share chose to send.
pub fn as_track_summary(track: &SharedCollectionTrack) -> CollectionTrackSummary {
    CollectionTrackSummary {
        id: track.id,
        name: track.name.clone(),
        activity_type: track.activity_type,
        distance_m: track.distance_m,
        ascent_m: track.ascent_m,
        descent_m: track.descent_m,
        has_recorded_trip: false,
    }
}

/// The recipient's waypoint as the owner's list and map take it.
pub fn as_waypoint(waypoint: &SharedWaypoint) -> Waypoint {
    Waypoint {
        id: waypoint.id,
        collection_track_id: None,
        name: waypoint.name.clone(),
        description: waypoint.description.clone(),
        lat: waypoint.lat,
        lon: waypoint.lon,
        ele: waypoint.ele,
        symbol: waypoint.symbol.clone(),
    }
}

/// Several shared collections: their tracks and waypoints on one map, and
/// the collections, each opening its page.
#[component]
pub fn SharedCollections(token: String, overview: ShareOverview) -> Element {
    let heading = title(&overview);
    let tracks: Vec<CollectionTrackSummary> = overview
        .collection_tracks
        .iter()
        .map(as_track_summary)
        .collect();
    rsx! {
        h1 { id: "share-title", "{heading}" }
        SharedMap { token: token.clone(), tracks, waypoints: None }
        div { class: "table-scroll",
            table { id: "shared-collections",
                thead {
                    tr {
                        th { scope: "col", "Collection" }
                        th { scope: "col", "Activity" }
                        th { scope: "col", class: "num", "Tracks" }
                        th { scope: "col", class: "num", "Waypoints" }
                    }
                }
                tbody {
                    for collection in overview.collections {
                        tr { key: "{collection.id}",
                            td {
                                Link {
                                    to: Route::SharedCollectionPage {
                                        token: token.clone(),
                                        id: collection.id,
                                    },
                                    "{collection.name}"
                                }
                            }
                            td {
                                if let Some(activity) = collection.activity_type {
                                    ActivityIcon { activity }
                                }
                            }
                            td { class: "num", "{collection.track_count}" }
                            td { class: "num", "{collection.waypoint_count}" }
                        }
                    }
                }
            }
        }
    }
}

/// Collection `id` of the share `overview` describes: its details, map,
/// tracks and waypoints. The share's title sits above it, leading back to
/// the share's list when it holds several collections.
#[component]
pub fn SharedCollectionView(token: String, id: i64, overview: ShareOverview) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    // The id travels back with the answer: the router shows the next
    // collection through this same scope.
    let collection = use_resource(use_reactive!(|id| async move {
        (id, api::get_shared_collection(&archive(), id).await)
    }));
    let waypoints = use_resource(use_reactive!(|id| async move {
        (
            id,
            api::list_shared_collection_waypoints(&archive(), id).await,
        )
    }));
    let many = overview.collections.len() > 1;
    let heading = title(&overview);

    rsx! {
        if many {
            p { class: "share-title",
                Link { to: Route::Shared { token: token.clone() }, "{heading}" }
            }
        } else if overview.label.is_some() {
            p { class: "share-title", "{heading}" }
        }
        match &*collection.read_unchecked() {
            Some((read_for, Ok(detail))) if *read_for == id => {
                let tracks: Vec<CollectionTrackSummary> =
                    detail.tracks.iter().map(as_track_summary).collect();
                rsx! {
                    hgroup {
                        h1 { id: "collection-name",
                            if let Some(activity) = detail.activity_type {
                                ActivityIcon { activity }
                            }
                            "{detail.name}"
                        }
                        p { id: "collection-counts",
                            {counts(detail.tracks.len(), detail.waypoint_count as usize, " · ")}
                        }
                    }
                    if !detail.description.is_empty() {
                        p { id: "collection-description", class: "collection-description",
                            "{detail.description}"
                        }
                    }
                    // Drawn once its waypoints are read, so they are read once.
                    match &*waypoints.read_unchecked() {
                        Some((read_for, read)) if *read_for == id => rsx! {
                            SharedMap {
                                token: token.clone(),
                                tracks: tracks.clone(),
                                waypoints: Some(drawn(read.clone())),
                            }
                        },
                        _ => rsx! {},
                    }
                    TrackList { tracks, token: token.clone() }
                    match &*waypoints.read_unchecked() {
                        Some((read_for, Ok(waypoints))) if *read_for == id => rsx! {
                            WaypointList {
                                waypoints: crate::collection::by_name(
                                    waypoints.iter().map(as_waypoint).collect(),
                                ),
                                linked: false,
                            }
                        },
                        _ => rsx! {},
                    }
                }
            }
            Some((read_for, Err(err))) if *read_for == id && err.is_not_found() => rsx! {
                p { class: "error", "This collection is not part of what was shared." }
            },
            Some((read_for, Err(err))) if *read_for == id => rsx! {
                p { class: "error", "Could not load this collection: {err}" }
            },
            _ => rsx! { p { "Loading…" } },
        }
    }
}

/// `tracks` on one map with `waypoints` — or, without them, every waypoint
/// the share reaches — and a click on a line opening its track's page under
/// the share.
#[component]
pub fn SharedMap(
    token: String,
    tracks: Vec<CollectionTrackSummary>,
    waypoints: Option<Vec<MapWaypoint>>,
) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let mut handle = use_signal(|| None::<document::Eval>);
    let _draw = use_resource(use_reactive!(|token, tracks, waypoints| async move {
        // A map without lines or markers beats no map at all: the list
        // below still leads to every track.
        let archive = archive();
        let ids: Vec<i64> = tracks.iter().map(|track| track.id).collect();
        let (positions, waypoints) = futures_util::future::join(
            api::list_collection_track_positions(&archive, &ids),
            async {
                match waypoints {
                    Some(waypoints) => waypoints,
                    None => drawn(api::list_share_waypoints(&archive).await),
                }
            },
        )
        .await;
        let positions = positions.unwrap_or_else(|err| {
            dioxus::logger::tracing::error!("could not read the tracks: {err}");
            Vec::new()
        });
        let mut map = interop::start_overview_map(
            collection_lines(&tracks, &positions),
            waypoints,
            Default::default(),
        );
        handle.set(Some(map));
        while let Ok(event) = map.recv::<OverviewEvent>().await {
            match event {
                OverviewEvent::Open(track) => {
                    navigator().push(track_route(Some(&token), track));
                }
                OverviewEvent::Hover(track) => {
                    if let Some(map) = handle.read().as_ref() {
                        interop::highlight_on_overview_map(map, track);
                    }
                }
            }
        }
    }));

    let shown: Vec<_> = tracks.iter().map(|track| track.activity_type).collect();
    rsx! {
        div { id: "overview-map", class: "overview-map" }
        ActivityLegend { shown }
    }
}

/// The waypoints as a map draws them; ones that could not be read cost
/// their markers, not the map.
pub fn drawn(read: Result<Vec<SharedWaypoint>, api::ApiError>) -> Vec<MapWaypoint> {
    crate::collection::drawn_waypoints(read.map(|read| read.iter().map(as_waypoint).collect()))
}
