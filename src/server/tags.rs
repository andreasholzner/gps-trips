//! Tagging trips (US-33) from the trip detail page. Kept separate from
//! `repo.rs` (DB-only) and `http.rs`, mirroring how `delete.rs`/`edit.rs`
//! isolate their own concerns rather than folding everything into one file.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use time::OffsetDateTime;

use crate::models::{normalize_tag_name, Tag, TagOverview};
use crate::server::{error::AppError, repo, repo::Rename, state::AppState};

/// The `POST /api/trips/:id/tags` request body (ADR-0008).
#[derive(Deserialize)]
pub struct AddTagRequest {
    name: String,
}

/// 404 if `id` isn't a trip; otherwise `Ok(())`. Every handler in this file
/// checks trip existence first, before any other validation, so a request
/// against a nonexistent trip always 404s regardless of what else is wrong
/// with it (e.g. an invalid tag name) — shared here so the check and that
/// ordering can't drift between handlers.
async fn require_trip(state: &AppState, id: i64) -> Result<(), AppError> {
    if repo::get_trip(&state.pool, id).await?.is_none() {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// GET `/api/trips/:id/tags` — a trip's current tags (US-33). 404 if the
/// trip doesn't exist.
pub async fn handle_list_trip_tags(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<Tag>>, AppError> {
    require_trip(&state, id).await?;
    Ok(Json(repo::list_trip_tags(&state.pool, id).await?))
}

/// POST `/api/trips/:id/tags` — tag a trip (US-33). 404 if the trip doesn't
/// exist, checked before `name` is validated so an invalid name against a
/// nonexistent trip still 404s (matching the other two handlers below)
/// rather than 400ing on the name first. `name` is normalized (trimmed,
/// lowercased) and rejected with 400 if it contains whitespace or is empty
/// after trimming. The tag is created on-demand if it doesn't exist yet;
/// tagging with an already-applied tag is a no-op, not an error.
pub async fn handle_add_trip_tag(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<AddTagRequest>,
) -> Result<(StatusCode, Json<Tag>), AppError> {
    require_trip(&state, id).await?;
    let name = normalize_tag_name(&body.name).map_err(AppError::BadRequest)?;
    let tag_id = repo::get_or_create_tag(&state.pool, &name).await?;
    repo::add_trip_tag(&state.pool, id, tag_id).await?;
    Ok((StatusCode::CREATED, Json(Tag { id: tag_id, name })))
}

/// DELETE `/api/trips/:id/tags/:tag_id` — remove a tag from a trip (US-33).
/// The `tag` row itself is left in place for reuse/autocomplete. 404 if the
/// trip doesn't exist; otherwise 204 whether or not the tag was actually
/// applied (idempotent removal).
pub async fn handle_remove_trip_tag(
    State(state): State<AppState>,
    Path((id, tag_id)): Path<(i64, i64)>,
) -> Result<StatusCode, AppError> {
    require_trip(&state, id).await?;
    repo::remove_trip_tag(&state.pool, id, tag_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET `/api/tags` — every tag that exists, for the trip detail page's
/// autocomplete suggestions (US-33).
pub async fn handle_list_all_tags(
    State(state): State<AppState>,
) -> Result<Json<Vec<Tag>>, AppError> {
    Ok(Json(repo::list_all_tags(&state.pool).await?))
}

/// GET `/api/tags/overview` — every tag with its trip counts and the active
/// summary shares naming it, for the Tags screen (US-83).
pub async fn handle_tag_overview(
    State(state): State<AppState>,
) -> Result<Json<Vec<TagOverview>>, AppError> {
    Ok(Json(
        repo::list_tag_overview(&state.pool, OffsetDateTime::now_utc()).await?,
    ))
}

/// POST `/api/tags` — create a tag carrying no trips yet (US-83), from the
/// same body a trip is tagged with. 400 for a name US-33 would refuse, with
/// its reason; 409 for a name that already exists, normalized.
pub async fn handle_create_tag(
    State(state): State<AppState>,
    Json(body): Json<AddTagRequest>,
) -> Result<(StatusCode, Json<Tag>), AppError> {
    let name = normalize_tag_name(&body.name).map_err(AppError::BadRequest)?;
    match repo::create_tag(&state.pool, &name).await? {
        Some(tag) => Ok((StatusCode::CREATED, Json(tag))),
        None => Err(AppError::Conflict(format!("tag \"{name}\" already exists"))),
    }
}

/// PATCH `/api/tags/:id` — rename a tag (US-85), from the same body a trip
/// is tagged with: every trip and summary share naming it carries the new
/// name. 200 with the renamed tag; 400 for a name US-33 would refuse, with
/// its reason; 404 for a tag that does not exist; 409 for a name another tag
/// has, normalized.
pub async fn handle_rename_tag(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<AddTagRequest>,
) -> Result<Json<Tag>, AppError> {
    let name = normalize_tag_name(&body.name).map_err(AppError::BadRequest)?;
    match repo::rename_tag(&state.pool, id, &name).await? {
        Rename::Renamed(tag) => Ok(Json(tag)),
        Rename::Missing => Err(AppError::NotFound),
        Rename::Taken => Err(AppError::Conflict(format!("tag \"{name}\" already exists"))),
    }
}

/// DELETE `/api/tags/:id` — delete a tag (US-83): it comes off every trip,
/// and every summary share naming it is narrowed, or stopped if it named
/// nothing else. 204; 404 for a tag that does not exist.
pub async fn handle_delete_tag(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    if repo::delete_tag(&state.pool, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

/// The `POST /api/trips/tags` request body (US-34, ADR-0008).
#[derive(Deserialize)]
pub struct BulkAddTagsRequest {
    trip_ids: Vec<i64>,
    names: Vec<String>,
}

/// POST `/api/trips/tags` — from the list page's multi-select, tag every trip
/// in `trip_ids` with every name in `names` in one request (US-34). 400 if
/// either list is empty. Trip existence is checked before names are
/// validated, mirroring `handle_add_trip_tag`'s single-trip ordering: if any
/// `trip_ids` entry doesn't exist, the whole request 404s and nothing is
/// created or linked. Every name is normalized (trimmed, lowercased); the
/// first invalid one 400s the whole request, same as the single-tag handler.
pub async fn handle_bulk_add_trip_tags(
    State(state): State<AppState>,
    Json(body): Json<BulkAddTagsRequest>,
) -> Result<Json<Vec<Tag>>, AppError> {
    if body.trip_ids.is_empty() {
        return Err(AppError::BadRequest("no trips selected".to_string()));
    }
    if body.names.is_empty() {
        return Err(AppError::BadRequest("no tags provided".to_string()));
    }
    if !repo::trips_exist(&state.pool, &body.trip_ids).await? {
        return Err(AppError::NotFound);
    }
    let names = body
        .names
        .iter()
        .map(|name| normalize_tag_name(name).map_err(AppError::BadRequest))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(
        repo::bulk_add_trip_tags(&state.pool, &body.trip_ids, &names).await?,
    ))
}
