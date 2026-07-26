//! Editing a trip's name and activity type (US-15). Kept separate from
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
