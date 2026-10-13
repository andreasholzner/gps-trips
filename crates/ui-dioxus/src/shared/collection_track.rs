//! Shared collection tracks (US-94), as their recipient sees them: several
//! on one map above their list, or one track's page — its map, elevation
//! profile, waypoints, figures and GPX — naming its collection only when the
//! share names that too. Never a tag or a control.

use dioxus::prelude::*;
use trip_archive_types::{CollectionTrackSummary, ShareOverview};

use super::collection::{as_track_summary, drawn, SharedMap};
use super::{title, ShareError};
use crate::activity_icon::ActivityIcon;
use crate::api::{self, ApiClient};
use crate::collection::TrackList;
use crate::collection_track::TrackStats;
use crate::detail::TrackViews;
use crate::Route;

/// `/s/:token/collection-tracks/:id` — one track of a share.
#[component]
pub fn SharedCollectionTrackPage(token: String, id: i64) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    let overview = use_resource(move || async move { api::share_overview(&archive()).await });

    match &*overview.read_unchecked() {
        None => rsx! { p { "Loading…" } },
        Some(Err(err)) => rsx! { ShareError { err: err.clone() } },
        Some(Ok(overview)) => rsx! {
            SharedCollectionTrackView { token, id, overview: overview.clone() }
        },
    }
}

/// Several shared tracks: on one map, and listed, each opening its page.
#[component]
pub fn SharedCollectionTracks(token: String, overview: ShareOverview) -> Element {
    let heading = title(&overview);
    let tracks: Vec<CollectionTrackSummary> = overview
        .collection_tracks
        .iter()
        .map(as_track_summary)
        .collect();
    rsx! {
        h1 { id: "share-title", "{heading}" }
        SharedMap { token: token.clone(), tracks: tracks.clone(), waypoints: None }
        TrackList { tracks, token }
    }
}

/// Whether the share's own page lists more than the one track a track's
/// page shows, and so is worth leading back to.
pub fn leads_back(overview: &ShareOverview) -> bool {
    !overview.collections.is_empty() || overview.collection_tracks.len() > 1
}

/// Track `id` of the share `overview` describes.
#[component]
pub fn SharedCollectionTrackView(token: String, id: i64, overview: ShareOverview) -> Element {
    let archive = use_context::<Signal<ApiClient>>();
    // The id travels back with each answer: the router shows the next track
    // through this same scope.
    let track = use_resource(use_reactive!(|id| async move {
        (id, api::get_shared_collection_track(&archive(), id).await)
    }));
    let geometry = use_resource(use_reactive!(|id| async move {
        let archive = archive();
        let (track, waypoints) = futures_util::future::join(
            api::get_collection_track_geometry(&archive, id),
            api::list_shared_track_waypoints(&archive, id),
        )
        .await;
        (id, track.map(|track| (track, drawn(waypoints))))
    }));
    let heading = title(&overview);

    rsx! {
        if leads_back(&overview) {
            p { class: "share-title",
                Link { to: Route::Shared { token: token.clone() }, "{heading}" }
            }
        } else if overview.label.is_some() {
            p { class: "share-title", "{heading}" }
        }
        match &*track.read_unchecked() {
            Some((read_for, Ok(detail))) if *read_for == id => rsx! {
                hgroup {
                    h1 { id: "trip-name",
                        ActivityIcon { activity: detail.track.activity_type }
                        "{detail.track.name}"
                    }
                    if let Some(collection) = &detail.collection {
                        p { id: "collection-of-track",
                            "From the collection "
                            Link {
                                to: Route::SharedCollectionPage {
                                    token: token.clone(),
                                    id: collection.id,
                                },
                                "{collection.name}"
                            }
                        }
                    }
                }
                TrackStats {
                    activity: detail.track.activity_type,
                    distance_m: detail.track.distance_m,
                    ascent_m: detail.track.ascent_m,
                    descent_m: detail.track.descent_m,
                }
                match &*geometry.read_unchecked() {
                    Some((read_for, Ok((geometry, waypoints)))) if *read_for == id => rsx! {
                        TrackViews {
                            track: geometry.clone(),
                            activity: detail.track.activity_type,
                            climbs: Vec::new(),
                            markers: Vec::new(),
                            waypoints: waypoints.clone(),
                            on_open_photos: |_| {},
                        }
                    },
                    Some((read_for, Err(err))) if *read_for == id => rsx! {
                        p { class: "error", "Could not load the track: {err}" }
                    },
                    _ => rsx! { p { "Loading the track…" } },
                }
                div { class: "trip-actions",
                    a {
                        class: "quiet",
                        href: api::collection_track_gpx_url(&archive(), id),
                        "Download GPX"
                    }
                }
            },
            Some((read_for, Err(err))) if *read_for == id && err.is_not_found() => rsx! {
                p { class: "error", "This track is not part of what was shared." }
            },
            Some((read_for, Err(err))) if *read_for == id => rsx! {
                p { class: "error", "Could not load this track: {err}" }
            },
            _ => rsx! { p { "Loading…" } },
        }
    }
}
