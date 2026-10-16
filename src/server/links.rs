//! Links from what the owner did to what they planned over the JSON API
//! (US-99, ADR-0008): a selection linked in one request, what something is
//! linked to read for its page, and a link removed from either end — every
//! link has a trip at one end, so removing one names that trip. And the
//! links the archive proposes (US-103, ADR-0030): listed, counted, confirmed
//! or dismissed a selection at a time, and looked for again on request.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};

use crate::models::{CreateLinks, CreatedLinks, LinkPairs, Links, ProposalCount, ProposedLink};
use crate::server::{
    error::AppError,
    link_check,
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
        .route("/api/link-proposals", get(handle_proposals))
        .route("/api/link-proposals/count", get(handle_proposal_count))
        .route("/api/link-proposals/confirm", post(handle_confirm))
        .route("/api/link-proposals/dismiss", post(handle_dismiss))
        .route("/api/trips/:id/link-check", post(handle_check_trip))
        .route(
            "/api/collections/:id/link-check",
            post(handle_check_collection),
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

/// `GET /api/link-proposals` — every proposal, as the Proposed links page
/// lists it.
async fn handle_proposals(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProposedLink>>, AppError> {
    Ok(Json(repo::list_proposals(&state.pool).await?))
}

/// `GET /api/link-proposals/count` — how many, for the navigation.
async fn handle_proposal_count(
    State(state): State<AppState>,
) -> Result<Json<ProposalCount>, AppError> {
    let count = repo::count_proposals(&state.pool).await?;
    Ok(Json(ProposalCount { count }))
}

/// `POST /api/link-proposals/confirm` — make every pair a link, all or
/// nothing. 400 for no pair; 404 if one is not a proposal, and then nothing
/// changed.
async fn handle_confirm(
    State(state): State<AppState>,
    Json(body): Json<LinkPairs>,
) -> Result<StatusCode, AppError> {
    settled(repo::confirm_proposals(&state.pool, non_empty(&body)?).await?)
}

/// `POST /api/link-proposals/dismiss` — drop every pair, all or nothing, as
/// confirming does.
async fn handle_dismiss(
    State(state): State<AppState>,
    Json(body): Json<LinkPairs>,
) -> Result<StatusCode, AppError> {
    settled(repo::dismiss_proposals(&state.pool, non_empty(&body)?).await?)
}

fn non_empty(body: &LinkPairs) -> Result<&[crate::models::LinkPair], AppError> {
    if body.pairs.is_empty() {
        return Err(AppError::BadRequest("No proposal selected".to_string()));
    }
    Ok(&body.pairs)
}

fn settled(all: bool) -> Result<StatusCode, AppError> {
    all.then_some(StatusCode::NO_CONTENT)
        .ok_or(AppError::NotFound)
}

/// `POST /api/trips/:id/link-check` — look for links for the trip, as if it
/// had just entered the archive. How many were proposed.
async fn handle_check_trip(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ProposalCount>, AppError> {
    found(link_check::check_trip(&state.pool, id).await?)
}

/// `POST /api/collections/:id/link-check` — look for links for the
/// collection's tracks and waypoints, likewise.
async fn handle_check_collection(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ProposalCount>, AppError> {
    found(link_check::check_collection(&state.pool, id).await?)
}

fn found(proposed: Option<usize>) -> Result<Json<ProposalCount>, AppError> {
    let count = proposed.ok_or(AppError::NotFound)?;
    Ok(Json(ProposalCount {
        count: count as i64,
    }))
}
