//! A trip's climbs (US-81), found again from its stored track under its
//! current activity whenever they are asked for, so an activity change shows
//! at once and nothing beyond the trip's figures is stored. Kept out of
//! `http.rs`, as `tracks.rs` keeps its own concern.

use axum::{
    extract::{Path, State},
    Json,
};

use crate::models::Climb;
use crate::server::{climbs, error::AppError, repo, state::AppState};

/// GET `/api/trips/:id/climbs` — the trip's climbs in track order. 404 for a
/// trip that does not exist.
pub async fn handle_list_climbs(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<Climb>>, AppError> {
    climbs_response(&state, id).await
}

/// The climbs of trip `id`, for whichever route has decided the caller may
/// have them — the owner's, or a share's that reaches the trip.
pub(crate) async fn climbs_response(
    state: &AppState,
    id: i64,
) -> Result<Json<Vec<Climb>>, AppError> {
    let trip = repo::get_trip(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let geojson = repo::get_track_geojson(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(climbs::of_track(&geojson, trip.activity_type)))
}
