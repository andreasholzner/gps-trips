use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::services::ServeDir;

use crate::models::{LocationSource, Photo, TripKind, TripSummary};
use crate::server::{
    delete,
    edit::handle_edit_trip,
    error::AppError,
    filter::{parse_filter, TripFilterQuery},
    import::{handle_add_photos, handle_import},
    komoot::KomootClient,
    komoot_sync::{self, SyncResultQuery},
    paths,
    render::{render_detail, render_import_form, render_sync_candidates, render_trip_list},
    repo,
    state::{self, AppState},
    tags::{
        handle_add_trip_tag, handle_list_all_tags, handle_list_trip_tags, handle_remove_trip_tag,
    },
};

/// The JSON shape returned by `GET /api/trips/:id/photos` (ADR-0008).
///
/// Wraps the DB `Photo` record and adds the public `url`/`thumbnail_url` the
/// client uses to fetch the image bytes. Constructed at the HTTP boundary so
/// the DB model stays a plain record with no HTTP concerns. `lat`/`lon`/
/// `location_source` (US-3) are derived once at import and persisted, so —
/// unlike `url` — they travel straight from `photo` with no extra constructor
/// argument (ADR-0015). `thumbnail_url` (US-5) is always populated — it falls
/// back to the full-size `url` when a photo has no thumbnail (generation
/// failed, or the photo predates US-5) — so the client never has to branch on
/// its absence.
#[derive(Serialize)]
struct PhotoResponse {
    id: i64,
    trip_id: i64,
    original_name: String,
    content_type: Option<String>,
    byte_len: i64,
    created_at: String,
    url: String,
    thumbnail_url: String,
    lat: Option<f64>,
    lon: Option<f64>,
    location_source: LocationSource,
}

impl PhotoResponse {
    fn from_photo(photo: Photo, url: String, thumbnail_url: String) -> Self {
        Self {
            id: photo.id,
            trip_id: photo.trip_id,
            original_name: photo.original_name,
            content_type: photo.content_type,
            byte_len: photo.byte_len,
            created_at: photo.created_at,
            url,
            thumbnail_url,
            lat: photo.lat,
            lon: photo.lon,
            location_source: photo.location_source,
        }
    }
}

/// Build the application router. Shared by `main` and the integration tests so
/// both exercise the exact same routing (ADR-0012).
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(trip_list))
        .route("/import", get(import_form))
        .route("/api/import", post(handle_import))
        // US-13: filtered list as JSON (same query params as `/`, ADR-0008/0011).
        .route("/api/trips", get(list_trips_api))
        .route("/trips/:id", get(trip_detail))
        .route("/api/trips/:id/gpx", get(download_gpx))
        .route("/api/trips/:id/track.geojson", get(track_geojson))
        // US-9/US-24: delete a trip and its photo blobs. US-15: edit its name/activity type.
        .route(
            "/api/trips/:id",
            axum::routing::delete(handle_delete_trip).patch(handle_edit_trip),
        )
        // US-2: attach photos later (POST) and list a trip's photos (GET).
        .route(
            "/api/trips/:id/photos",
            post(handle_add_photos).get(list_trip_photos),
        )
        // US-33: tag a trip (POST/GET) and untag it (DELETE).
        .route(
            "/api/trips/:id/tags",
            post(handle_add_trip_tag).get(handle_list_trip_tags),
        )
        .route(
            "/api/trips/:id/tags/:tag_id",
            axum::routing::delete(handle_remove_trip_tag),
        )
        .route("/api/tags", get(handle_list_all_tags))
        // US-22: review + trigger a Komoot "Sync now" pull.
        .route("/komoot/sync", get(sync_candidates_page))
        .route("/api/komoot/sync", post(handle_sync))
        // US-7: serve photo blobs stored by the BlobStore (ADR-0007).
        // The wildcard captures the blob key so any backend's url_for works here.
        .route("/media/*path", get(serve_media))
        // Vendored, self-hosted map/chart assets and glue (ADR-0005/0006, US-10).
        // Resolved relative to the executable, not the CWD, so "binary + adjacent
        // public/ folder" is a deployable unit startable from anywhere (ADR-0016).
        .nest_service("/static", ServeDir::new(paths::assets_dir()))
        .with_state(state)
}

/// GET `/` — the trip list, the archive's home (US-6), optionally narrowed by
/// the filter query parameters (US-13, ADR-0011) and split into a Recorded/
/// Planned tab (US-32). Unlike every other filter dimension, `kind` always
/// resolves to a concrete value here — the page shows exactly one tab's worth
/// of trips, defaulting to Recorded when `?kind=` is absent.
async fn trip_list(
    State(state): State<AppState>,
    Query(query): Query<TripFilterQuery>,
) -> Result<Html<String>, AppError> {
    let mut filter = parse_filter(&query)?;
    let active_kind = filter.trip_kind.unwrap_or(TripKind::Recorded);
    filter.trip_kind = Some(active_kind);
    let trips = repo::list_trips(&state.pool, &filter).await?;
    Ok(Html(render_trip_list(&trips, &query, active_kind)))
}

/// GET `/api/trips` — the same filtered trip list as JSON (US-13, ADR-0008/0011),
/// for a future non-HTML client (US-16). Lightweight rows only, no track geometry.
async fn list_trips_api(
    State(state): State<AppState>,
    Query(query): Query<TripFilterQuery>,
) -> Result<Json<Vec<TripSummary>>, AppError> {
    let filter = parse_filter(&query)?;
    Ok(Json(repo::list_trips(&state.pool, &filter).await?))
}

/// GET `/import` — the import form (US-1: the owner uploads a GPX file).
async fn import_form() -> Html<String> {
    Html(render_import_form())
}

/// The `POST /api/komoot/sync` request body (ADR-0008): the tours the owner
/// checked on the review page, in submission order — each carrying the `kind`
/// the page knew it was (US-29), so the pull lists only the endpoint(s) the
/// selection spans.
#[derive(Deserialize)]
struct SyncRequest {
    tours: Vec<komoot_sync::SelectedTour>,
}

/// The `POST /api/komoot/sync` response: how many pending edits/deletes were
/// pushed and tours were pulled/imported, and which trip/tour (if any)
/// halted the run and in which phase — the client redirects to the review
/// page with these as query params (US-20/US-22/US-24, no session/flash
/// mechanism here).
#[derive(Serialize)]
struct SyncResponse {
    pushed: usize,
    /// US-24: tours deleted on Komoot this run.
    deleted: usize,
    imported: usize,
    failed_tour: Option<String>,
    failed_msg: Option<String>,
    failed_phase: Option<&'static str>,
}

/// `state.komoot`, or a clear 400 if the app booted without
/// `KOMOOT_EMAIL`/`KOMOOT_PASSWORD` set (`main.rs`: an optional integration,
/// not a hard startup requirement).
fn require_komoot(state: &AppState) -> Result<Arc<dyn KomootClient>, AppError> {
    state.komoot.clone().ok_or_else(|| {
        AppError::BadRequest(
            "Komoot sync is not configured (set KOMOOT_EMAIL/KOMOOT_PASSWORD)".to_string(),
        )
    })
}

/// GET `/komoot/sync` — the "Sync now" review page (US-20/US-22): lists every
/// Komoot tour not yet imported, for the owner to select from, plus how many
/// trips have an edit pending to push.
async fn sync_candidates_page(
    State(state): State<AppState>,
    Query(result): Query<SyncResultQuery>,
) -> Result<Html<String>, AppError> {
    let client = require_komoot(&state)?;
    let candidates = komoot_sync::list_sync_candidates(&state.pool, client).await?;
    let pending_edit_count = repo::komoot::count_edit_pending(&state.pool).await?;
    Ok(Html(render_sync_candidates(
        &candidates,
        pending_edit_count,
        &result,
    )))
}

/// POST `/api/komoot/sync` — push pending edits, then push pending deletes,
/// then import the owner's selected tours (US-20/US-22/US-24), in that
/// order (ADR-0021: push, then pull). A failure in either push step halts
/// before anything later is even attempted; every phase halts on its own
/// first failure.
///
/// Claims `state`'s sync flag for the duration of the run (US-26): a second
/// concurrent sync request is rejected with 409 rather than racing this
/// one's push phase, and `PATCH`/`DELETE /api/trips/:id` are rejected the
/// same way while this run is in flight. The claimed `SyncGuard` releases
/// the flag when it drops — on every return path below, success or an
/// early `?` error — so a halted sync (US-25) never leaves the app
/// permanently locked out of edits/deletes.
async fn handle_sync(
    State(state): State<AppState>,
    Json(body): Json<SyncRequest>,
) -> Result<Json<SyncResponse>, AppError> {
    let client = require_komoot(&state)?;
    let _sync_guard = state
        .try_start_sync()
        .ok_or_else(|| AppError::Conflict(state::SYNC_IN_PROGRESS_MSG.to_string()))?;

    let push_summary = komoot_sync::push_pending_edits(&state.pool, Arc::clone(&client)).await?;
    if let Some((tour_id, msg)) = push_summary.failed {
        return Ok(Json(SyncResponse {
            pushed: push_summary.pushed.len(),
            deleted: 0,
            imported: 0,
            failed_tour: Some(tour_id),
            failed_msg: Some(msg),
            failed_phase: Some("push"),
        }));
    }

    let delete_summary =
        komoot_sync::push_pending_deletes(&state.pool, Arc::clone(&client)).await?;
    if let Some((tour_id, msg)) = delete_summary.failed {
        return Ok(Json(SyncResponse {
            pushed: push_summary.pushed.len(),
            deleted: delete_summary.deleted.len(),
            imported: 0,
            failed_tour: Some(tour_id),
            failed_msg: Some(msg),
            failed_phase: Some("push"),
        }));
    }

    let summary =
        komoot_sync::sync_selected_tours(&state.pool, &state.store, client, &body.tours).await?;
    let failed_phase = summary.failed.is_some().then_some("pull");
    Ok(Json(SyncResponse {
        pushed: push_summary.pushed.len(),
        deleted: delete_summary.deleted.len(),
        imported: summary.imported.len(),
        failed_tour: summary.failed.as_ref().map(|(tour_id, _)| tour_id.clone()),
        failed_msg: summary.failed.map(|(_, msg)| msg),
        failed_phase,
    }))
}

/// GET `/trips/:id` — the trip detail page (the redirect target after import).
async fn trip_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Html<String>, AppError> {
    let trip = repo::get_trip(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Html(render_detail(&trip)))
}

/// GET `/api/trips/:id/gpx` — download the original uploaded GPX file
/// byte-for-byte, named after the trip (US-21).
async fn download_gpx(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let gpx = repo::get_original_gpx(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;

    let headers = [
        (header::CONTENT_TYPE, "application/gpx+xml".to_string()),
        (
            header::CONTENT_DISPOSITION,
            gpx_content_disposition(&gpx.name),
        ),
    ];
    Ok((headers, gpx.bytes).into_response())
}

/// GET `/api/trips/:id/track.geojson` — the track geometry as GeoJSON (US-7).
/// The client fetches this once to draw both the map polyline and the elevation
/// chart (geometry + elevation/distance arrays travel together; ADR-0005/0006).
async fn track_geojson(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let geojson = repo::get_track_geojson(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let headers = [(header::CONTENT_TYPE, "application/geo+json")];
    Ok((headers, geojson).into_response())
}

/// DELETE `/api/trips/:id` — delete a trip and its photo blobs (US-9, the v1
/// API surface fixed by ADR-0008). Removes the trip row (cascading to
/// `track`/`photo`) and best-effort removes each photo's blob. If the trip
/// is Komoot-sourced, its link row is marked `delete_pending` in the same
/// transaction (US-24) rather than dropped — the next "Sync now" push phase
/// deletes it on Komoot too. 404 if no such trip exists; 204 with an empty
/// body on success. 409 if a "Sync now" run is in flight (US-26) — it would
/// otherwise race the push phase's read of `delete_pending`.
async fn handle_delete_trip(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    if state.sync_in_progress() {
        return Err(AppError::Conflict(state::SYNC_IN_PROGRESS_MSG.to_string()));
    }
    let deleted = delete::delete_trip(&state.pool, &state.store, id).await?;
    if !deleted {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// GET `/api/trips/:id/photos` — the trip's photos as JSON (US-2/US-7).
/// Each photo includes a `url` the gallery can use to fetch the image bytes.
/// 404 if the trip does not exist.
async fn list_trip_photos(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<PhotoResponse>>, AppError> {
    if repo::get_trip(&state.pool, id).await?.is_none() {
        return Err(AppError::NotFound);
    }
    let photos = repo::list_photos(&state.pool, id)
        .await?
        .into_iter()
        .map(|p| {
            let url = state.store.url_for(&p.blob_key);
            let thumbnail_url = p
                .thumbnail_key
                .as_deref()
                .map(|k| state.store.url_for(k))
                .unwrap_or_else(|| url.clone());
            PhotoResponse::from_photo(p, url, thumbnail_url)
        })
        .collect();
    Ok(Json(photos))
}

/// GET `/media/*path` — serve a photo blob from the `BlobStore` (US-7).
/// The path is the blob key as emitted by `BlobStore::url_for`. Returns 404
/// when the key does not exist, 500 for any other I/O error.
async fn serve_media(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Response, AppError> {
    let content_type = content_type_from_path(&path);
    let store = Arc::clone(&state.store);
    let bytes = tokio::task::spawn_blocking(move || store.get(&path))
        .await
        .expect("blob store task panicked")
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::NotFound
            } else {
                AppError::Storage(e)
            }
        })?;
    Ok(([(header::CONTENT_TYPE, content_type)], bytes).into_response())
}

/// Derive a MIME type from a blob key's file extension. Falls back to
/// `application/octet-stream` for anything unrecognised.
fn content_type_from_path(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    }
}

/// Build an RFC 6266-compliant `Content-Disposition` for the GPX download:
/// a plain ASCII `filename` fallback plus a UTF-8 `filename*`, so non-ASCII trip
/// names (e.g. Norwegian "Tromsø") download with their real name in modern
/// browsers while older ones still get a usable name.
fn gpx_content_disposition(trip_name: &str) -> String {
    let filename = format!("{}.gpx", sanitize_filename(trip_name));
    let ascii_fallback: String = filename
        .chars()
        .map(|c| if c.is_ascii() { c } else { '_' })
        .collect();
    format!(
        "attachment; filename=\"{ascii_fallback}\"; filename*=UTF-8''{}",
        rfc5987_encode(&filename)
    )
}

/// Make a trip name safe as a filename: drop control characters and the bytes
/// that would break a header or imply a path. Keeps Unicode (handled by
/// `filename*`). Falls back to `trip` if nothing usable remains.
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | '"') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "trip".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Percent-encode a string for the `filename*` parameter value per RFC 5987:
/// keep the `attr-char` set literal, encode every other byte as `%XX`.
fn rfc5987_encode(s: &str) -> String {
    const ATTR_CHARS: &[u8] = b"!#$&+-.^_`|~";
    let mut out = String::with_capacity(s.len());
    for &byte in s.as_bytes() {
        if byte.is_ascii_alphanumeric() || ATTR_CHARS.contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // US-21: the download filename derived from the trip name.

    #[test]
    fn us21_disposition_uses_trip_name_for_ascii_names() {
        let cd = gpx_content_disposition("Oslo Hills Walk");
        assert!(
            cd.contains(r#"filename="Oslo Hills Walk.gpx""#),
            "ASCII fallback should be the trip name; got: {cd}"
        );
        assert!(
            cd.contains("filename*=UTF-8''Oslo%20Hills%20Walk.gpx"),
            "got: {cd}"
        );
    }

    #[test]
    fn us21_disposition_handles_non_ascii_names() {
        // "Tromsø" — ø is UTF-8 0xC3 0xB8.
        let cd = gpx_content_disposition("Tromsø");
        assert!(
            cd.contains(r#"filename="Troms_.gpx""#),
            "non-ASCII chars become _ in the ASCII fallback; got: {cd}"
        );
        assert!(
            cd.contains("filename*=UTF-8''Troms%C3%B8.gpx"),
            "filename* must be RFC-5987 percent-encoded UTF-8; got: {cd}"
        );
    }

    #[test]
    fn sanitize_filename_replaces_path_and_quote_characters() {
        assert_eq!(sanitize_filename("a/b\\c\"d"), "a_b_c_d");
    }

    #[test]
    fn sanitize_filename_falls_back_when_empty() {
        assert_eq!(sanitize_filename("   "), "trip");
    }
}
