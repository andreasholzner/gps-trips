//! Adding files to a collection, or making one of several (US-88), over the
//! JSON API (ADR-0008): the upload is parked, reviewed as often as the owner
//! changes a decision, and confirmed in one transaction (ADR-0029).
//!
//! The server proposes and the owner decides: every review works the whole
//! upload out again from the parked files, what the collection holds now and
//! the decisions so far (`collection_match`), so a decision on an earlier file
//! changes what a later one is proposed. A single file for a new collection
//! still takes US-86's preview and create (`collections`).

use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use time::OffsetDateTime;

use crate::config;
use crate::models::{
    CollectionFile, CollectionImportReview, ConfirmCollectionImport, CreatedCollection, HeldItem,
    ImportDecisions, ReviewItem, ReviewMatch,
};
use crate::server::{
    collection_gpx::{parse_collection, ParsedCollection},
    collection_match::{match_files, Decisions, Held, Matched, Matching, Origin},
    collections::{fallback_name, optional_activity},
    db,
    error::{AppError, ImportError},
    repo::{self, GpxDownload, NewCollectionFile},
    state::AppState,
    track_files::gpx_download,
};

mod plan;

/// The routes, merged into the owner's router behind its gate. The upload
/// takes the import's body cap: a whole collection's file is several megabytes.
pub fn router() -> Router<AppState> {
    let upload_limit =
        axum::extract::DefaultBodyLimit::max(config::server::PHOTO_IMPORT_BODY_LIMIT);
    Router::new()
        .route(
            "/api/collection-imports",
            post(handle_upload).layer(upload_limit),
        )
        .route(
            "/api/collection-imports/:id",
            axum::routing::delete(handle_cancel),
        )
        .route("/api/collection-imports/:id/review", post(handle_review))
        .route("/api/collection-imports/:id/confirm", post(handle_confirm))
        .route("/api/collections/:id/files", get(handle_files))
        .route("/api/collection-files/:id/gpx", get(handle_file_gpx))
}

/// The name a file is kept under: its own without any path, else the
/// default, so a collection named after it is called "Collection".
pub(crate) fn file_name_of(raw: Option<&str>) -> String {
    let name = raw
        .unwrap_or_default()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim();
    if name.is_empty() {
        "Collection.gpx".to_string()
    } else {
        name.to_string()
    }
}

/// The files in the order they are taken: by their names (US-88).
fn in_name_order(files: &mut [(String, Vec<u8>)]) {
    files.sort_by(|(a, _), (b, _)| a.to_lowercase().cmp(&b.to_lowercase()).then(a.cmp(b)));
}

/// Every file read, or the first one refused, named.
fn parse_all(files: &[(String, Vec<u8>)]) -> Result<Vec<ParsedCollection>, ImportError> {
    files
        .iter()
        .map(|(name, bytes)| {
            parse_collection(bytes).map_err(|reason| ImportError::InFile {
                file: name.clone(),
                reason: Box::new(reason),
            })
        })
        .collect()
}

/// The files read and matched, off the async runtime (ADR-0004).
struct Worked {
    parsed: Vec<ParsedCollection>,
    held: Held,
    matching: Matching,
}

async fn work(
    files: Vec<(String, Vec<u8>)>,
    held: Held,
    decisions: Decisions,
) -> Result<(Vec<(String, Vec<u8>)>, Worked), AppError> {
    tokio::task::spawn_blocking(move || {
        let parsed = parse_all(&files)?;
        let matching = match_files(&held, &parsed, &decisions);
        Ok((
            files,
            Worked {
                parsed,
                held,
                matching,
            },
        ))
    })
    .await
    .map_err(|e| AppError::Internal(format!("matching the files failed: {e}")))?
}

async fn held_now(state: &AppState, collection_id: Option<i64>) -> Result<Held, AppError> {
    match collection_id {
        Some(id) => Ok(repo::load_held(&mut *state.pool.acquire().await?, id).await?),
        None => Ok(Held::default()),
    }
}

/// `POST /api/collection-imports` — one or more `gpx` files and, to add them
/// to an existing collection, its `collection_id`. Parks them and answers
/// with the review of the proposals. Nothing in the archive changes. 400 for
/// no file, 404 for no such collection, 422 naming the first file that is
/// not GPX or holds nothing to collect.
async fn handle_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<CollectionImportReview>, AppError> {
    let bad = |e: axum::extract::multipart::MultipartError| AppError::BadRequest(e.to_string());
    let mut files = Vec::new();
    let mut collection_id = None;
    while let Some(field) = multipart.next_field().await.map_err(bad)? {
        match field.name() {
            Some("gpx") => {
                let name = file_name_of(field.file_name());
                files.push((name, field.bytes().await.map_err(bad)?.to_vec()));
            }
            Some("collection_id") => {
                let text = field.text().await.map_err(bad)?;
                let text = text.trim();
                if !text.is_empty() {
                    let id = text.parse::<i64>().map_err(|_| {
                        AppError::BadRequest(format!("'{text}' is not a collection id"))
                    })?;
                    collection_id = Some(id);
                }
            }
            _ => {}
        }
    }
    if files.is_empty() {
        return Err(AppError::BadRequest("Missing 'gpx' field".to_string()));
    }
    if let Some(id) = collection_id {
        if !repo::collection_exists(&state.pool, id).await? {
            return Err(AppError::NotFound);
        }
    }
    in_name_order(&mut files);

    let held = held_now(&state, collection_id).await?;
    let (files, worked) = work(files, held, Decisions::default()).await?;

    // Abandoned uploads only pile up when uploads happen (US-12's rule).
    let now = OffsetDateTime::now_utc();
    repo::sweep_collection_imports(&state.pool, now - config::server::STAGED_IMPORT_TTL).await?;
    let kept: Vec<NewCollectionFile> = files
        .iter()
        .map(|(file_name, gpx)| NewCollectionFile { file_name, gpx })
        .collect();
    let staging_id = repo::insert_collection_import(&state.pool, collection_id, &kept, now)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(review_of(
        staging_id,
        collection_id,
        &files,
        &worked,
        &ImportDecisions::default(),
    )))
}

/// `POST /api/collection-imports/:id/review` — the review again, with the
/// owner's decisions so far. 404 once the upload is spent, swept or
/// cancelled, or its collection deleted.
async fn handle_review(
    State(state): State<AppState>,
    Path(staging_id): Path<i64>,
    Json(decisions): Json<ImportDecisions>,
) -> Result<Json<CollectionImportReview>, AppError> {
    let staged = repo::get_collection_import(&state.pool, staging_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let held = held_now(&state, staged.collection_id).await?;
    let (files, worked) = work(staged.files, held, Decisions::from(&decisions)).await?;
    Ok(Json(review_of(
        staging_id,
        staged.collection_id,
        &files,
        &worked,
        &decisions,
    )))
}

/// `POST /api/collection-imports/:id/confirm` — apply the decisions, all of
/// them or none, and spend the upload. 201 with the collection's id; 404 for
/// a spent upload; 409 when the collection changed since the review the owner
/// decided on, and the upload stays for another one; 400 for an unknown
/// activity, or a new collection with everything skipped.
async fn handle_confirm(
    State(state): State<AppState>,
    Path(staging_id): Path<i64>,
    Json(confirm): Json<ConfirmCollectionImport>,
) -> Result<Response, AppError> {
    let activity_type = optional_activity(confirm.activity_type.clone())?;
    let prepared = prepare(&state, staging_id, &confirm).await?;
    let id = store(&state, prepared, activity_type).await?;
    Ok((StatusCode::CREATED, Json(CreatedCollection { id })).into_response())
}

const CHANGED: &str = "The collection changed since these files were reviewed; review them again";

/// A confirmation worked out from the parked files before the write lock is
/// taken: matching a whole collection takes about a second, which every other
/// writer would otherwise wait out.
struct Prepared {
    staging_id: i64,
    collection_id: Option<i64>,
    /// What the collection held when the files were matched.
    held_digest: String,
    files: Vec<(String, Vec<u8>)>,
    worked: Worked,
    /// The collection's name — for a new one, the owner's or the suggested
    /// one — and a new one's description.
    name: String,
    description: String,
    /// The place of the last track the collection keeps; added ones go after.
    held_last: i64,
}

/// Read the parked files and what the collection holds, refuse if it is not
/// what the owner reviewed, and match the files with their decisions.
async fn prepare(
    state: &AppState,
    staging_id: i64,
    confirm: &ConfirmCollectionImport,
) -> Result<Prepared, AppError> {
    let staged = repo::get_collection_import(&state.pool, staging_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let held = held_now(state, staged.collection_id).await?;
    if held.digest() != confirm.held_digest {
        return Err(AppError::Conflict(CHANGED.to_string()));
    }
    let held_digest = confirm.held_digest.clone();
    let (files, worked) = work(staged.files, held, Decisions::from(&confirm.decisions)).await?;
    // US-86 makes no collection of nothing; the upload stays for another choice.
    let outcome = &worked.matching.outcome;
    if staged.collection_id.is_none() && outcome.tracks.is_empty() && outcome.waypoints.is_empty() {
        return Err(AppError::BadRequest(
            "Every track and waypoint is skipped: there is nothing to make a collection of"
                .to_string(),
        ));
    }

    let (name, description, held_last) = match staged.collection_id {
        Some(id) => {
            let name = repo::collection_name(&state.pool, id)
                .await?
                .ok_or(AppError::NotFound)?;
            let held_last = if outcome.replace_all {
                0
            } else {
                repo::last_collection_track_position(&state.pool, id).await?
            };
            (name, String::new(), held_last)
        }
        None => {
            let name = confirm
                .name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map_or_else(
                    || fallback_name(&worked.parsed[0], &files[0].0),
                    str::to_string,
                );
            let description = confirm.description.as_deref().map_or_else(
                || worked.parsed[0].suggested_description(),
                |d| d.trim().to_string(),
            );
            (name, description, 0)
        }
    };
    Ok(Prepared {
        staging_id,
        collection_id: staged.collection_id,
        held_digest,
        files,
        worked,
        name,
        description,
        held_last,
    })
}

/// Store a prepared confirmation in one transaction: spend the upload, check
/// once more that the collection is what was matched, and apply the
/// outcome. Returns the collection's id.
async fn store(
    state: &AppState,
    prepared: Prepared,
    activity_type: Option<crate::models::ActivityType>,
) -> Result<i64, AppError> {
    let Prepared {
        staging_id,
        collection_id,
        held_digest,
        files,
        worked,
        name,
        description,
        held_last,
    } = prepared;
    let derived = plan::derive(&worked, &name, held_last);
    let tracks = plan::tracks(&worked, &derived);
    let waypoints = plan::waypoints(&worked);
    let kept: Vec<NewCollectionFile> = files
        .iter()
        .map(|(file_name, gpx)| NewCollectionFile { file_name, gpx })
        .collect();
    let outcome = &worked.matching.outcome;

    let mut tx = db::begin_write(&state.pool).await?;
    repo::take_collection_import_in_tx(&mut tx, staging_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let id = match collection_id {
        Some(id) => {
            // Changed while the files were matched: rolled back, so the
            // upload stays for another review.
            if repo::load_held(&mut tx, id).await?.digest() != held_digest {
                return Err(AppError::Conflict(CHANGED.to_string()));
            }
            id
        }
        None => {
            repo::insert_collection_in_tx(
                &mut tx,
                &repo::NewCollection {
                    name: &name,
                    description: &description,
                    activity_type,
                    files: &[],
                    tracks: &[],
                    waypoints: &[],
                },
            )
            .await?
        }
    };
    repo::apply_collection_import(
        &mut tx,
        id,
        &repo::CollectionImportPlan {
            // A new collection holds nothing to replace.
            replace_all: outcome.replace_all && collection_id.is_some(),
            remove_tracks: &outcome.remove_tracks,
            remove_waypoints: &outcome.remove_waypoints,
            tracks: &tracks,
            waypoints: &waypoints,
            files: &kept,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// `DELETE /api/collection-imports/:id` — the owner cancelled; nothing
/// changes but the upload going.
async fn handle_cancel(
    State(state): State<AppState>,
    Path(staging_id): Path<i64>,
) -> Result<StatusCode, AppError> {
    match repo::delete_collection_import(&state.pool, staging_id).await? {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(AppError::NotFound),
    }
}

/// `GET /api/collections/:id/files` — the files the collection was imported
/// and added from, in the order they came.
async fn handle_files(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<CollectionFile>>, AppError> {
    if !repo::collection_exists(&state.pool, id).await? {
        return Err(AppError::NotFound);
    }
    Ok(Json(repo::list_collection_files(&state.pool, id).await?))
}

/// `GET /api/collection-files/:id/gpx` — one kept file, byte for byte, under
/// its name (as US-21's download for a trip).
async fn handle_file_gpx(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let (file_name, bytes) = repo::get_collection_file(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    // The download appends `.gpx` itself.
    let name = match file_name.len().checked_sub(4) {
        Some(stem)
            if file_name.is_char_boundary(stem)
                && file_name[stem..].eq_ignore_ascii_case(".gpx") =>
        {
            file_name[..stem].to_string()
        }
        _ => file_name,
    };
    Ok(gpx_download(GpxDownload { name, bytes }))
}

/// The review as the screen shows it.
fn review_of(
    staging_id: i64,
    collection_id: Option<i64>,
    files: &[(String, Vec<u8>)],
    worked: &Worked,
    decisions: &ImportDecisions,
) -> CollectionImportReview {
    let Worked {
        parsed,
        held,
        matching,
    } = worked;
    let track_name = |origin: Origin| match origin {
        Origin::Held(id) => held
            .tracks
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.name.clone()),
        Origin::File { file, index } => parsed[file].tracks[index].name.clone(),
    };
    let waypoint_name = |origin: Origin| match origin {
        Origin::Held(id) => held
            .waypoints
            .iter()
            .find(|w| w.id == id)
            .and_then(|w| w.name.clone()),
        Origin::File { file, index } => parsed[file].waypoints[index].name.clone(),
    };
    let item = |matched: &Matched, name: &dyn Fn(Origin) -> Option<String>| ReviewItem {
        file: matched.file,
        index: matched.index,
        name: name(Origin::File {
            file: matched.file,
            index: matched.index,
        }),
        matched: matched.matched.map(|origin| ReviewMatch {
            name: name(origin),
            file: match origin {
                Origin::Held(_) => None,
                Origin::File { file, .. } => Some(file),
            },
        }),
        proposed: matched.proposed,
        action: matched.action,
        choices: matched.choices().to_vec(),
    };
    let held_item = |id: i64, name: Option<String>, removed: &[i64]| HeldItem {
        id,
        name,
        remove: removed.contains(&id),
    };

    CollectionImportReview {
        staging_id,
        collection_id,
        suggested_name: fallback_name(&parsed[0], &files[0].0),
        suggested_description: parsed[0].suggested_description(),
        files: files.iter().map(|(name, _)| name.clone()).collect(),
        replace_all: decisions.replace_all && collection_id.is_some(),
        tracks: matching
            .tracks
            .iter()
            .map(|m| item(m, &track_name))
            .collect(),
        waypoints: matching
            .waypoints
            .iter()
            .map(|m| item(m, &waypoint_name))
            .collect(),
        held_tracks: matching
            .unmatched_tracks
            .iter()
            .map(|&id| held_item(id, track_name(Origin::Held(id)), &decisions.remove_tracks))
            .collect(),
        held_waypoints: matching
            .unmatched_waypoints
            .iter()
            .map(|&id| {
                held_item(
                    id,
                    waypoint_name(Origin::Held(id)),
                    &decisions.remove_waypoints,
                )
            })
            .collect(),
        held_digest: held.digest(),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
