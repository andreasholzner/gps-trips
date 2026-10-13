//! Tagging trips (US-33), collections and their tracks (US-93), and the Tags
//! screen's tags (US-83/85). Kept separate from
//! `repo.rs` (DB-only) and `http.rs`, mirroring how `delete.rs`/`edit.rs`
//! isolate their own concerns rather than folding everything into one file.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;
use time::OffsetDateTime;

use crate::models::{normalize_tag_name, Tag, TagOverview};
use crate::server::{
    error::AppError,
    repo::{self, Rename, Tagged},
    state::AppState,
};

/// The `POST …/:id/tags` request body (ADR-0008).
#[derive(Deserialize)]
pub struct AddTagRequest {
    name: String,
}

/// The routes tagging what `tagged` makes of the id in `prefix/:id`: a trip
/// (US-33), a collection or a collection track (US-93), each the same way —
/// `GET`/`POST prefix/:id/tags` lists and adds, `DELETE
/// prefix/:id/tags/:tag_id` removes.
pub fn links(prefix: &str, tagged: fn(i64) -> Tagged) -> Router<AppState> {
    Router::new()
        .route(
            &format!("{prefix}/:id/tags"),
            get(move |State(state): State<AppState>, Path(id): Path<i64>| {
                list_tags(state, tagged(id))
            })
            .post(
                move |State(state): State<AppState>,
                      Path(id): Path<i64>,
                      Json(body): Json<AddTagRequest>| {
                    add_tag(state, tagged(id), body)
                },
            ),
        )
        .route(
            &format!("{prefix}/:id/tags/:tag_id"),
            delete(
                move |State(state): State<AppState>, Path((id, tag_id)): Path<(i64, i64)>| {
                    remove_tag(state, tagged(id), tag_id)
                },
            ),
        )
}

/// 404 if what is tagged does not exist; otherwise `Ok(())`. Every link
/// handler checks this first, before any other validation, so a request
/// against something that does not exist always 404s regardless of what
/// else is wrong with it (e.g. an invalid tag name).
async fn require(state: &AppState, tagged: Tagged) -> Result<(), AppError> {
    if !repo::tagged_exists(&state.pool, tagged).await? {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// `GET …/:id/tags` — the tags it carries.
async fn list_tags(state: AppState, tagged: Tagged) -> Result<Json<Vec<Tag>>, AppError> {
    require(&state, tagged).await?;
    Ok(Json(repo::list_tags_of(&state.pool, tagged).await?))
}

/// `POST …/:id/tags` — tag it. `name` is normalized (trimmed, lowercased)
/// and rejected with 400 if it contains whitespace or a comma, or is empty
/// after trimming. The tag is created on-demand if it doesn't exist yet;
/// tagging with an already-applied tag is a no-op, not an error.
async fn add_tag(
    state: AppState,
    tagged: Tagged,
    body: AddTagRequest,
) -> Result<(StatusCode, Json<Tag>), AppError> {
    require(&state, tagged).await?;
    let name = normalize_tag_name(&body.name).map_err(AppError::BadRequest)?;
    // Checked again where the link is made: it may have been deleted since.
    let tag = repo::add_tag_named(&state.pool, tagged, &name)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok((StatusCode::CREATED, Json(tag)))
}

/// `DELETE …/:id/tags/:tag_id` — untag it. The `tag` row itself is left in
/// place for reuse/autocomplete; 204 whether or not the tag was applied
/// (idempotent removal).
async fn remove_tag(state: AppState, tagged: Tagged, tag_id: i64) -> Result<StatusCode, AppError> {
    require(&state, tagged).await?;
    repo::remove_tag(&state.pool, tagged, tag_id).await?;
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
/// in `trip_ids` with every name in `names` in one request (US-34).
pub async fn handle_bulk_add_trip_tags(
    State(state): State<AppState>,
    Json(body): Json<BulkAddTagsRequest>,
) -> Result<Json<Vec<Tag>>, AppError> {
    if body.trip_ids.is_empty() {
        return Err(AppError::BadRequest("no trips selected".to_string()));
    }
    let tagged: Vec<Tagged> = body.trip_ids.iter().copied().map(Tagged::Trip).collect();
    bulk_add(&state, &tagged, &body.names).await
}

/// The `POST /api/collections/tags` request body (US-100, ADR-0008).
#[derive(Deserialize)]
pub struct BulkAddCollectionTagsRequest {
    collection_ids: Vec<i64>,
    names: Vec<String>,
}

/// POST `/api/collections/tags` — tag every collection selected on the
/// Collections page or the trip list's Collections tab with every name in
/// `names` in one request (US-100). Never their tracks (US-93).
pub async fn handle_bulk_add_collection_tags(
    State(state): State<AppState>,
    Json(body): Json<BulkAddCollectionTagsRequest>,
) -> Result<Json<Vec<Tag>>, AppError> {
    if body.collection_ids.is_empty() {
        return Err(AppError::BadRequest("no collections selected".to_string()));
    }
    let tagged: Vec<Tagged> = body
        .collection_ids
        .iter()
        .copied()
        .map(Tagged::Collection)
        .collect();
    bulk_add(&state, &tagged, &body.names).await
}

/// Tag everything in `tagged` with every name in `names`, all or nothing.
/// 400 if `names` is empty or any of them is invalid — each normalized
/// (trimmed, lowercased) as the single-tag handler does; 404 if anything
/// selected does not exist, and then nothing is created or linked.
async fn bulk_add(
    state: &AppState,
    tagged: &[Tagged],
    names: &[String],
) -> Result<Json<Vec<Tag>>, AppError> {
    if names.is_empty() {
        return Err(AppError::BadRequest("no tags provided".to_string()));
    }
    let names = names
        .iter()
        .map(|name| normalize_tag_name(name).map_err(AppError::BadRequest))
        .collect::<Result<Vec<_>, _>>()?;
    repo::bulk_add_tags(&state.pool, tagged, &names)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}
