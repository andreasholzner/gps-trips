//! Editing a trip's name and activity type (US-15). Kept separate from
//! `repo.rs` (DB-only) and `http.rs`, mirroring how `delete.rs` isolates its
//! one write operation instead of folding every concern into one file.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::server::{
    error::AppError,
    import::resolve_activity_type,
    repo,
    state::{AppState, SYNC_IN_PROGRESS_MSG},
};

/// The `PATCH /api/trips/:id` request body (ADR-0008). Both fields are
/// optional: an omitted field leaves that column unchanged, so the owner can
/// edit just the name, just the activity type, or both in one call.
#[derive(Deserialize)]
pub struct EditTripRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    activity_type: Option<String>,
}

/// `PATCH /api/trips/:id` — edit a trip's name and/or activity type (US-15).
/// 404 if the trip doesn't exist. A given `name` must be non-blank (400
/// otherwise) — unlike import's `resolve_name`, there is no GPX/date fallback
/// to fall back to when editing an existing trip. A given `activity_type` is
/// validated by the same `resolve_activity_type` import already uses (blank
/// resets to `Unknown`; an unrecognized value is a 400). 409 if a "Sync now"
/// run is in flight (US-26) — it would otherwise race the push phase's read
/// of `edit_pending`.
///
/// Validates the request body first, then writes both fields in one atomic
/// `repo::update_trip` call (each field `None` if omitted) instead of
/// fetching the trip first and merging in Rust — that read-then-write shape
/// would leave a window for a concurrent edit or delete of the same trip to
/// race against; existence is instead read off `update_trip`'s own
/// `rows_affected` result, with no separate query.
pub async fn handle_edit_trip(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<EditTripRequest>,
) -> Result<StatusCode, AppError> {
    if state.sync_in_progress() {
        return Err(AppError::Conflict(SYNC_IN_PROGRESS_MSG.to_string()));
    }
    let name = match body.name {
        Some(name) if !name.trim().is_empty() => Some(name),
        Some(_) => return Err(AppError::BadRequest("name cannot be empty".to_string())),
        None => None,
    };
    let activity_type = match body.activity_type {
        Some(value) => Some(resolve_activity_type(Some(value))?),
        None => None,
    };

    let updated = repo::update_trip(&state.pool, id, name.as_deref(), activity_type).await?;
    if !updated {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
