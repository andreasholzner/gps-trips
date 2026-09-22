//! Editing a trip's name and activity type (US-15), and many trips'
//! activity type at once (US-63). Kept separate from
//! `repo.rs` (DB-only) and `http.rs`, mirroring how `delete.rs` isolates its
//! one write operation instead of folding every concern into one file.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::models::KomootPrivacy;
use crate::server::{
    error::AppError,
    import::resolve_activity_type,
    repo,
    state::{AppState, SYNC_IN_PROGRESS_MSG},
};

/// The `PATCH /api/trips/:id` request body (ADR-0008). Every field is
/// optional: an omitted field is left unchanged, so the owner can edit just
/// the name, just the activity type, just the Komoot privacy, or any
/// combination in one call.
#[derive(Deserialize)]
pub struct EditTripRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    activity_type: Option<String>,
    /// US-35: the linked Komoot tour's privacy. Only `KomootPrivacy::SELECTABLE`
    /// values are accepted — see `resolve_privacy`.
    #[serde(default)]
    privacy_status: Option<String>,
}

/// Validate a requested privacy (US-35): one of the values the owner may
/// actually choose, or a 400. `unknown` is deliberately rejected even though
/// it parses — it's a display-only state for a Komoot value this app couldn't
/// map, never something to push back (ADR-0021).
fn resolve_privacy(value: &str) -> Result<KomootPrivacy, AppError> {
    value
        .parse::<KomootPrivacy>()
        .ok()
        .filter(|p| KomootPrivacy::SELECTABLE.contains(p))
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "privacy_status must be one of: {}",
                KomootPrivacy::SELECTABLE
                    .iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
}

/// `PATCH /api/trips/:id` — edit a trip's name, activity type and/or the
/// privacy of its linked Komoot tour (US-15/US-35). 404 if the trip doesn't
/// exist. A given `name` must be non-blank (400 otherwise) — unlike import's
/// `resolve_name`, there is no GPX/date fallback to fall back to when editing
/// an existing trip. A given `activity_type` is validated by the same
/// `resolve_activity_type` import already uses (blank resets to `Unknown`; an
/// unrecognized value is a 400). A given `privacy_status` must name a
/// settable privacy (400 otherwise) *and* the trip must be Komoot-sourced —
/// there is no tour whose privacy an unlinked trip's edit could change, so
/// that combination is a 400 rather than a silent no-op. 409 if a "Sync now"
/// run is in flight (US-26) — it would otherwise race the push phase's read
/// of `edit_pending`.
///
/// Validates the request body first, then writes every field in one atomic
/// `repo::update_trip` call (each field `None` if omitted) instead of
/// fetching the trip first and merging in Rust — that read-then-write shape
/// would leave a window for a concurrent edit or delete of the same trip to
/// race against; existence is instead read off `update_trip`'s own
/// `rows_affected` result, with no separate query. The one exception is the
/// Komoot-link check above — a single `EXISTS`, and only when a privacy was
/// actually requested. A nonexistent trip therefore answers 400 rather than
/// 404 for a request that carries `privacy_status`; such a request is wrong
/// on either count.
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
    let privacy = match &body.privacy_status {
        Some(value) => Some(resolve_privacy(value)?),
        None => None,
    };
    if privacy.is_some() && !repo::komoot::link_exists(&state.pool, id).await? {
        return Err(AppError::BadRequest(
            "privacy_status can only be set on a trip linked to a Komoot tour".to_string(),
        ));
    }

    let updated = repo::update_trip(
        &state.pool,
        id,
        &repo::TripEdit {
            name: name.as_deref(),
            activity_type,
            privacy,
        },
    )
    .await?;
    if !updated {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The `POST /api/trips/activity_type` request body (US-63): the trips
/// selected on the list screen, and the one activity type they all get.
#[derive(Deserialize)]
pub struct BulkActivityRequest {
    trip_ids: Vec<i64>,
    activity_type: String,
}

/// `POST /api/trips/activity_type` — set one activity type on every selected
/// trip (US-63), mirroring `POST /api/trips/tags` (US-34): one request, one
/// transaction, and an unknown trip 404s the whole request with nothing
/// changed. The activity is validated as the single-trip edit validates it
/// (blank resets to `Unknown`, an unrecognized value is a 400, ADR-0018).
/// Unlike a tag, an activity type overwrites, so every linked trip's change
/// is queued for Komoot (US-22) — and, like the single-trip `PATCH`, the
/// request is refused with 409 while a sync is in flight (US-26).
pub async fn handle_bulk_set_activity_type(
    State(state): State<AppState>,
    Json(body): Json<BulkActivityRequest>,
) -> Result<StatusCode, AppError> {
    if state.sync_in_progress() {
        return Err(AppError::Conflict(SYNC_IN_PROGRESS_MSG.to_string()));
    }
    if body.trip_ids.is_empty() {
        return Err(AppError::BadRequest("no trips selected".to_string()));
    }
    let activity_type = resolve_activity_type(Some(body.activity_type))?;
    if !repo::set_activity_type(&state.pool, &body.trip_ids, activity_type).await? {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
