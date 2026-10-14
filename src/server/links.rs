//! Links from what the owner did to what they planned over the JSON API
//! (US-99, ADR-0008): a selection linked in one request, what something is
//! linked to read for its page, and a link removed from either end — every
//! link has a trip at one end, so removing one names that trip.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};

use crate::models::{CreateLinks, CreatedLinks, Links};
use crate::server::{
    error::AppError,
    repo::{self, LinkEnd, LinkOutcome},
    state::AppState,
};

/// The link routes, merged into the owner's router behind its gate.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/links", post(handle_create))
        .route(
            "/api/trips/:id/links",
            get(|state: State<AppState>, Path(id): Path<i64>| list(state, LinkEnd::Trip(id))),
        )
        .route(
            "/api/collection-tracks/:id/links",
            get(|state: State<AppState>, Path(id): Path<i64>| {
                list(state, LinkEnd::CollectionTrack(id))
            }),
        )
        .route(
            "/api/waypoints/:id/links",
            get(|state: State<AppState>, Path(id): Path<i64>| list(state, LinkEnd::Waypoint(id))),
        )
        .route(
            "/api/trips/:id/links/trips/:other",
            delete(
                |state: State<AppState>, Path((id, other)): Path<(i64, i64)>| {
                    remove(state, id, LinkEnd::Trip(other))
                },
            ),
        )
        .route(
            "/api/trips/:id/links/collection-tracks/:other",
            delete(
                |state: State<AppState>, Path((id, other)): Path<(i64, i64)>| {
                    remove(state, id, LinkEnd::CollectionTrack(other))
                },
            ),
        )
        .route(
            "/api/trips/:id/links/waypoints/:other",
            delete(
                |state: State<AppState>, Path((id, other)): Path<(i64, i64)>| {
                    remove(state, id, LinkEnd::Waypoint(other))
                },
            ),
        )
}

/// `POST /api/links` — link every pair the selection holds that can be
/// linked, all or nothing. 400 if it holds no such pair; 404 if anything in
/// it does not exist, and then nothing is linked.
async fn handle_create(
    State(state): State<AppState>,
    Json(body): Json<CreateLinks>,
) -> Result<Json<CreatedLinks>, AppError> {
    match repo::link_selection(&state.pool, &body.trip_ids, &body.collection_track_ids).await? {
        LinkOutcome::Linked { pairs } => Ok(Json(CreatedLinks { pairs })),
        LinkOutcome::NothingToLink => Err(AppError::BadRequest(
            "Nothing selected can be linked: select a recorded trip with planned trips or \
             collection tracks, or a planned trip with collection tracks"
                .to_string(),
        )),
        LinkOutcome::Missing => Err(AppError::NotFound),
    }
}

/// `GET …/:id/links` — what it is linked to.
async fn list(State(state): State<AppState>, end: LinkEnd) -> Result<Json<Links>, AppError> {
    repo::list_links(&state.pool, end)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

/// `DELETE /api/trips/:id/links/…` — remove the trip's link to the other
/// end. 204 whether or not they were linked; 404 if the trip does not exist.
async fn remove(
    State(state): State<AppState>,
    trip_id: i64,
    other: LinkEnd,
) -> Result<StatusCode, AppError> {
    if !repo::trips_exist(&state.pool, &[trip_id]).await? {
        return Err(AppError::NotFound);
    }
    repo::unlink(&state.pool, trip_id, other).await?;
    Ok(StatusCode::NO_CONTENT)
}
