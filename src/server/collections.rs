//! Collections over the JSON API (US-86, ADR-0008): importing one
//! from a GPX file, reading, editing and deleting it and its tracks, and the
//! waypoints and positions its page draws. Collections are records of their
//! own, apart from trips (ADR-0028); the track files share the trips' code
//! (`track_files`).

use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};

use crate::config;
use crate::models::{
    ActivityType, CollectionCount, CollectionDetail, CollectionEdit, CollectionPreview,
    CollectionSummary, CollectionTrackDetail, CollectionTrackEdit, CreatedCollection, TripTrack,
    Waypoint,
};
use crate::server::{
    collection_gpx::{parse_collection, track_name, ParsedCollection},
    collection_import::file_name_of,
    error::AppError,
    filter::parse_optional_bbox,
    geojson::build_track_geojson,
    gpx::compute_stats,
    import::resolve_activity_type,
    repo::{self, CollectionChange, CollectionTrackChange},
    state::AppState,
    track_files::{geojson_track, gpx_download},
    tracks::{parse_ids, TracksQuery},
};

/// The collection routes, merged into the owner's router behind its gate.
/// The uploads take the import's body cap: a whole collection's file is several
/// megabytes.
pub fn router() -> Router<AppState> {
    let upload_limit =
        axum::extract::DefaultBodyLimit::max(config::server::PHOTO_IMPORT_BODY_LIMIT);
    Router::new()
        .route(
            "/api/collections",
            get(handle_list).post(handle_create).layer(upload_limit),
        )
        .route("/api/collections/count", get(handle_count))
        .route(
            "/api/collections/preview",
            post(handle_preview).layer(upload_limit),
        )
        .route(
            "/api/collections/:id",
            get(handle_get).patch(handle_edit).delete(handle_delete),
        )
        .route("/api/collections/:id/tracks", get(handle_tracks))
        .route("/api/collections/:id/waypoints", get(handle_waypoints))
        .route("/api/collection-tracks", get(handle_list_tracks))
        .route(
            "/api/collection-tracks/:id",
            get(handle_get_track)
                .patch(handle_edit_track)
                .delete(handle_delete_track),
        )
        .route(
            "/api/collection-tracks/:id/track.geojson",
            get(handle_track_geojson),
        )
        .route("/api/collection-tracks/:id/gpx", get(handle_track_gpx))
        .route(
            "/api/collection-tracks/:id/waypoints",
            get(handle_track_waypoints),
        )
}

/// A collection's upload: the file, its name, and the details the owner
/// gave, each `None` when the form left it out.
#[derive(Default)]
struct Upload {
    gpx: Vec<u8>,
    file_name: String,
    name: Option<String>,
    description: Option<String>,
    activity_type: Option<String>,
}

async fn read_upload(mut multipart: Multipart) -> Result<Upload, AppError> {
    let bad = |e: axum::extract::multipart::MultipartError| AppError::BadRequest(e.to_string());
    let mut upload = Upload::default();
    let mut has_gpx = false;
    while let Some(field) = multipart.next_field().await.map_err(bad)? {
        match field.name() {
            Some("gpx") => {
                upload.file_name = file_name_of(field.file_name());
                upload.gpx = field.bytes().await.map_err(bad)?.to_vec();
                has_gpx = true;
            }
            Some("name") => upload.name = Some(field.text().await.map_err(bad)?),
            Some("description") => upload.description = Some(field.text().await.map_err(bad)?),
            Some("activity_type") => upload.activity_type = Some(field.text().await.map_err(bad)?),
            _ => {}
        }
    }
    if !has_gpx {
        return Err(AppError::BadRequest("Missing 'gpx' field".to_string()));
    }
    Ok(upload)
}

/// An activity field as a collection or a collection track stores it: blank
/// or `unknown` is none at all.
pub(crate) fn optional_activity(value: Option<String>) -> Result<Option<ActivityType>, AppError> {
    Ok(Some(resolve_activity_type(value)?).filter(|activity| *activity != ActivityType::Unknown))
}

/// `POST /api/collections/preview` — what the file holds and what its
/// details are suggested as, read before anything is created. 422 for a file
/// that is not GPX or holds neither a track nor a waypoint.
async fn handle_preview(multipart: Multipart) -> Result<Json<CollectionPreview>, AppError> {
    let upload = read_upload(multipart).await?;
    let parsed = parse_collection(&upload.gpx)?;
    Ok(Json(CollectionPreview {
        suggested_name: parsed.suggested_name(&upload.file_name),
        suggested_description: parsed.suggested_description(),
        track_count: parsed.tracks.len(),
        waypoint_count: parsed.waypoints.len(),
    }))
}

/// `POST /api/collections` — import a GPX file as a new collection
/// of all its tracks and waypoints, in one transaction. A blank name falls
/// back to the suggested one, a description left out to the suggested one,
/// and a blank activity to none. 201 with the new collection's id; 422 for a
/// file with nothing to collect, and nothing is created.
async fn handle_create(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<Response, AppError> {
    let upload = read_upload(multipart).await?;
    let activity_type = optional_activity(upload.activity_type)?;
    let parsed = parse_collection(&upload.gpx)?;
    let name = upload
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| fallback_name(&parsed, &upload.file_name));
    let description = upload
        .description
        .map_or_else(|| parsed.suggested_description(), |d| d.trim().to_string());

    let derived: Vec<_> = parsed
        .tracks
        .iter()
        .map(|track| {
            (
                track_name(track.name.as_deref(), &name, track.position),
                compute_stats(&track.points),
                build_track_geojson(&track.points),
            )
        })
        .collect();
    let tracks: Vec<repo::NewCollectionTrack> = derived
        .iter()
        .zip(&parsed.tracks)
        .map(|((name, stats, geojson), track)| repo::NewCollectionTrack {
            name,
            stats,
            geojson,
            gpx: &track.gpx,
        })
        .collect();
    let on_track = parsed.waypoint_track();
    let waypoints: Vec<repo::NewWaypoint> = parsed
        .waypoints
        .iter()
        .map(|wp| repo::NewWaypoint {
            name: wp.name.as_deref(),
            description: wp.description.as_deref(),
            lat: wp.lat,
            lon: wp.lon,
            ele: wp.ele,
            symbol: wp.symbol.as_deref(),
            track: on_track,
        })
        .collect();

    let id = repo::insert_collection(
        &state.pool,
        &repo::NewCollection {
            name: &name,
            description: &description,
            activity_type,
            files: &[repo::NewCollectionFile {
                file_name: &upload.file_name,
                gpx: &upload.gpx,
            }],
            tracks: &tracks,
            waypoints: &waypoints,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(CreatedCollection { id })).into_response())
}

/// The name a collection gets when the owner gave none and the file
/// suggests none either.
pub(crate) fn fallback_name(parsed: &ParsedCollection, file_name: &str) -> String {
    let suggested = parsed.suggested_name(file_name);
    if suggested.is_empty() {
        "Collection".to_string()
    } else {
        suggested
    }
}

/// The `GET /api/collections` query: the trip list's name search and region
/// (US-87), under the trip list's own parameter names (ADR-0008).
#[derive(serde::Deserialize)]
struct ListQuery {
    q: Option<String>,
    bbox: Option<String>,
}

/// `GET /api/collections?q=…&bbox=…` — every collection matching the name
/// search and the region, alphabetically (US-87). Blank narrows nothing; 400
/// for a malformed region.
async fn handle_list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<CollectionSummary>>, AppError> {
    let region = parse_optional_bbox(query.bbox.as_deref())?;
    let name = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
    Ok(Json(
        repo::list_collections(&state.pool, name, region).await?,
    ))
}

/// `GET /api/collections/count` — how many collections there are, for the
/// Collections tab's count line (US-87), without reading what they hold.
async fn handle_count(State(state): State<AppState>) -> Result<Json<CollectionCount>, AppError> {
    let count = repo::count_collections(&state.pool).await?;
    Ok(Json(CollectionCount { count }))
}

/// `GET /api/collection-tracks?ids=…` — each requested collection track's
/// positions, as `[lon, lat]`, for the trip list's map to draw (US-87). A
/// track there is not is left out; 400 for ids that are not integers.
async fn handle_list_tracks(
    State(state): State<AppState>,
    Query(query): Query<TracksQuery>,
) -> Result<Json<Vec<TripTrack>>, AppError> {
    let ids = parse_ids(query.ids())?;
    if ids.is_empty() {
        return Ok(Json(Vec::new()));
    }
    Ok(Json(
        repo::list_collection_tracks_by_id(&state.pool, &ids).await?,
    ))
}

/// `GET /api/collections/:id` — the details and the tracks in file order.
async fn handle_get(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<CollectionDetail>, AppError> {
    let collection = repo::get_collection(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(collection))
}

/// A given name must not be blank: unlike at import, there is nothing to
/// fall back to.
pub(crate) fn edited_name(name: Option<String>) -> Result<Option<String>, AppError> {
    match name {
        Some(name) if name.trim().is_empty() => {
            Err(AppError::BadRequest("name cannot be empty".to_string()))
        }
        name => Ok(name.map(|name| name.trim().to_string())),
    }
}

/// `PATCH /api/collections/:id` — change the name, description or activity
/// (US-86). A blank activity takes it away; every track without one of its
/// own follows. 400 for a blank name or an unknown activity, 404 for no
/// such collection.
async fn handle_edit(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(edit): Json<CollectionEdit>,
) -> Result<StatusCode, AppError> {
    let name = edited_name(edit.name)?;
    let activity_type = edit
        .activity_type
        .map(|value| optional_activity(Some(value)))
        .transpose()?;
    let description = edit.description.map(|d| d.trim().to_string());
    let change = CollectionChange {
        name: name.as_deref(),
        description: description.as_deref(),
        activity_type,
    };
    if !repo::update_collection(&state.pool, id, &change).await? {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/collections/:id` — the collection with all its tracks and
/// waypoints.
async fn handle_delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    if !repo::delete_collection(&state.pool, id).await? {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/collections/:id/tracks` — every track's positions, for drawing
/// them on one map. 404 for no such collection.
async fn handle_tracks(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<TripTrack>>, AppError> {
    exists(&state, id).await?;
    Ok(Json(
        repo::list_collection_track_positions(&state.pool, id).await?,
    ))
}

/// `GET /api/collections/:id/waypoints` — every waypoint, in file order.
async fn handle_waypoints(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<Waypoint>>, AppError> {
    exists(&state, id).await?;
    Ok(Json(
        repo::list_collection_waypoints(&state.pool, id).await?,
    ))
}

async fn exists(state: &AppState, id: i64) -> Result<(), AppError> {
    if !repo::collection_exists(&state.pool, id).await? {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// `GET /api/collection-tracks/:id` — one track's page.
async fn handle_get_track(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<CollectionTrackDetail>, AppError> {
    let track = repo::get_collection_track(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(track))
}

/// `PATCH /api/collection-tracks/:id` — change the track's name or its own
/// activity, which overrides its collection's; a blank activity takes the
/// collection's again.
async fn handle_edit_track(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(edit): Json<CollectionTrackEdit>,
) -> Result<StatusCode, AppError> {
    let name = edited_name(edit.name)?;
    let activity_type = edit
        .activity_type
        .map(|value| optional_activity(Some(value)))
        .transpose()?;
    let change = CollectionTrackChange {
        name: name.as_deref(),
        activity_type,
    };
    if !repo::update_collection_track(&state.pool, id, &change).await? {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/collection-tracks/:id` — one track; its waypoints stay with
/// the collection.
async fn handle_delete_track(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    if !repo::delete_collection_track(&state.pool, id).await? {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/collection-tracks/:id/track.geojson` — the geometry, as a
/// trip's is read.
async fn handle_track_geojson(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let geojson = repo::get_collection_track_geojson(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(geojson_track(&geojson))
}

/// `GET /api/collection-tracks/:id/gpx` — this track alone as GPX, named
/// after it.
async fn handle_track_gpx(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let gpx = repo::get_collection_track_gpx(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(gpx_download(gpx))
}

/// `GET /api/collection-tracks/:id/waypoints` — the waypoints that belong to
/// the track.
async fn handle_track_waypoints(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<Waypoint>>, AppError> {
    repo::get_collection_track(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(
        repo::list_collection_track_waypoints(&state.pool, id).await?,
    ))
}
