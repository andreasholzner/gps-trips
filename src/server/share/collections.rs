//! US-94 — the recipient's routes for a share of collections or collection
//! tracks, under `/s/:token` like the rest. Each checks that what it is asked
//! for is something the share reaches, and answers the same 404 as for
//! something that does not exist otherwise. The paths are the owner's, so
//! the UI's client reads them unchanged through a share's client.

use std::collections::HashSet;

use axum::{
    extract::{Path, Query, State},
    response::Response,
    routing::get,
    Extension, Json, Router,
};

use super::share_id;
use crate::models::{
    CollectionDetail, Principal, SharedCollectionDetail, SharedCollectionTrack,
    SharedCollectionTrackDetail, SharedWaypoint, TripTrack,
};
use crate::server::{
    error::AppError,
    repo,
    state::AppState,
    track_files::{geojson_track, gpx_download},
    tracks::{parse_ids, TracksQuery},
};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/s/:token/api/share/waypoints", get(waypoints))
        .route("/s/:token/api/collections/:id", get(collection))
        .route(
            "/s/:token/api/collections/:id/waypoints",
            get(collection_waypoints),
        )
        .route("/s/:token/api/collection-tracks", get(tracks))
        .route("/s/:token/api/collection-tracks/:id", get(track))
        .route(
            "/s/:token/api/collection-tracks/:id/track.geojson",
            get(track_geojson),
        )
        .route("/s/:token/api/collection-tracks/:id/gpx", get(track_gpx))
        .route(
            "/s/:token/api/collection-tracks/:id/waypoints",
            get(track_waypoints),
        )
}

/// `track_id`, if the share reaches it; the same 404 as a track that does
/// not exist otherwise.
async fn covered_track(
    state: &AppState,
    principal: Principal,
    track_id: i64,
) -> Result<i64, AppError> {
    if repo::share_covers_collection_track(&state.pool, share_id(principal)?, track_id).await? {
        Ok(track_id)
    } else {
        Err(AppError::NotFound)
    }
}

/// `collection_id`, if the share names it; the same 404 as a collection
/// that does not exist otherwise.
async fn covered_collection(
    state: &AppState,
    principal: Principal,
    collection_id: i64,
) -> Result<i64, AppError> {
    if repo::share_covers_collection(&state.pool, share_id(principal)?, collection_id).await? {
        Ok(collection_id)
    } else {
        Err(AppError::NotFound)
    }
}

/// GET `/s/:token/api/share/waypoints` — every waypoint the share reaches,
/// for its overview map.
async fn waypoints(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Vec<SharedWaypoint>>, AppError> {
    Ok(Json(
        repo::list_shared_waypoints(&state.pool, share_id(principal)?).await?,
    ))
}

/// GET `/s/:token/api/collections/:id` — a shared collection's page.
async fn collection(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((_, id)): Path<(String, i64)>,
) -> Result<Json<SharedCollectionDetail>, AppError> {
    let id = covered_collection(&state, principal, id).await?;
    let collection = repo::get_collection(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(shared_collection(collection)))
}

/// GET `/s/:token/api/collections/:id/waypoints` — a shared collection's
/// waypoints, in file order.
async fn collection_waypoints(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((_, id)): Path<(String, i64)>,
) -> Result<Json<Vec<SharedWaypoint>>, AppError> {
    repo::list_shared_collection_waypoints(&state.pool, share_id(principal)?, id)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

/// GET `/s/:token/api/collection-tracks?ids=…` — the positions of the
/// shared tracks asked for, in one request. One the share does not reach is
/// absent, exactly as one that does not exist.
async fn tracks(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Query(query): Query<TracksQuery>,
) -> Result<Json<Vec<TripTrack>>, AppError> {
    let reached: HashSet<i64> =
        repo::list_shared_collection_tracks(&state.pool, share_id(principal)?)
            .await?
            .into_iter()
            .map(|track| track.id)
            .collect();
    let ids: Vec<i64> = parse_ids(query.ids())?
        .into_iter()
        .filter(|id| reached.contains(id))
        .collect();
    if ids.is_empty() {
        return Ok(Json(Vec::new()));
    }
    Ok(Json(
        repo::list_collection_tracks_by_id(&state.pool, &ids).await?,
    ))
}

/// GET `/s/:token/api/collection-tracks/:id` — a shared track's page.
async fn track(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((_, id)): Path<(String, i64)>,
) -> Result<Json<SharedCollectionTrackDetail>, AppError> {
    repo::shared_collection_track(&state.pool, share_id(principal)?, id)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

/// GET `/s/:token/api/collection-tracks/:id/track.geojson`.
async fn track_geojson(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((_, id)): Path<(String, i64)>,
) -> Result<Response, AppError> {
    let id = covered_track(&state, principal, id).await?;
    let geojson = repo::get_collection_track_geojson(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(geojson_track(&geojson))
}

/// GET `/s/:token/api/collection-tracks/:id/gpx`.
async fn track_gpx(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((_, id)): Path<(String, i64)>,
) -> Result<Response, AppError> {
    let id = covered_track(&state, principal, id).await?;
    let gpx = repo::get_collection_track_gpx(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(gpx_download(gpx))
}

/// GET `/s/:token/api/collection-tracks/:id/waypoints` — the waypoints that
/// belong to a shared track.
async fn track_waypoints(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((_, id)): Path<(String, i64)>,
) -> Result<Json<Vec<SharedWaypoint>>, AppError> {
    repo::list_shared_track_waypoints(&state.pool, share_id(principal)?, id)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

/// The recipient's copy of a collection, field by field — so a field the
/// owner's type grows later reaches a share only by being added here.
fn shared_collection(collection: CollectionDetail) -> SharedCollectionDetail {
    SharedCollectionDetail {
        id: collection.id,
        name: collection.name,
        description: collection.description,
        activity_type: collection.activity_type,
        waypoint_count: collection.waypoint_count,
        tracks: collection
            .tracks
            .into_iter()
            .map(|track| SharedCollectionTrack {
                id: track.id,
                name: track.name,
                activity_type: track.activity_type,
                distance_m: track.distance_m,
                ascent_m: track.ascent_m,
                descent_m: track.descent_m,
            })
            .collect(),
    }
}
