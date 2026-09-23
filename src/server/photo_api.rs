//! A trip's photos as the API serves them (US-2/US-7), and placing one on
//! the map by hand (US-30). Split from `http.rs`, which routes to these, the
//! way `edit.rs` and `delete.rs` hold their own handlers.

use axum::{
    extract::{Path, State},
    Json,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::models::{Photo, PhotoPlacement, PhotoResponse};
use crate::server::{
    error::AppError,
    repo,
    state::{AppState, SYNC_IN_PROGRESS_MSG},
    timezone,
};

/// GET `/api/trips/:id/photos` — the trip's photos as JSON (US-2/US-7).
/// Each photo includes a `url` the gallery can use to fetch the image bytes.
/// 404 if the trip does not exist.
pub async fn list_trip_photos(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<PhotoResponse>>, AppError> {
    let trip = repo::get_trip(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let photos = repo::list_photos(&state.pool, id)
        .await?
        .into_iter()
        .map(|p| respond(&state, p, trip.tz_name.as_deref()))
        .collect();
    Ok(Json(photos))
}

/// PATCH `/api/trips/:id/photos/:photo_id` — put the photo where the owner
/// placed it on the map (US-30), whatever position it had; the warning
/// before overwriting an automatic one is the screen's. Answers with the
/// photo as the list serves it. 400 for a position off the globe, 404 when
/// the trip has no such photo, 409 while a "Sync now" run is in flight — the
/// rule US-26 set for every `PATCH`.
pub async fn handle_place_photo(
    State(state): State<AppState>,
    Path((id, photo_id)): Path<(i64, i64)>,
    Json(body): Json<PhotoPlacement>,
) -> Result<Json<PhotoResponse>, AppError> {
    if state.sync_in_progress() {
        return Err(AppError::Conflict(SYNC_IN_PROGRESS_MSG.to_string()));
    }
    if !(-90.0..=90.0).contains(&body.lat) {
        return Err(AppError::BadRequest(
            "lat must be between -90 and 90".to_string(),
        ));
    }
    if !(-180.0..=180.0).contains(&body.lon) {
        return Err(AppError::BadRequest(
            "lon must be between -180 and 180".to_string(),
        ));
    }
    let photo = repo::place_photo(&state.pool, id, photo_id, body.lat, body.lon)
        .await?
        .ok_or(AppError::NotFound)?;
    let trip = repo::get_trip(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(respond(&state, photo, trip.tz_name.as_deref())))
}

/// A stored photo in its wire shape: the serving URLs from the blob store,
/// and the offset its caption is shown in.
fn respond(state: &AppState, photo: Photo, tz_name: Option<&str>) -> PhotoResponse {
    let url = state.store.url_for(&photo.blob_key);
    let thumbnail_url = photo
        .thumbnail_key
        .as_deref()
        .map(|k| state.store.url_for(k))
        .unwrap_or_else(|| url.clone());
    let taken_offset_secs = taken_offset(&photo, tz_name);
    PhotoResponse::from_photo(photo, url, thumbnail_url, taken_offset_secs)
}

/// The offset a photo's caption is shown in (US-62, `timezone::photo_offset`),
/// in seconds — `None` for a photo with no capture time to take it at.
fn taken_offset(photo: &Photo, tz_name: Option<&str>) -> Option<i32> {
    let taken_at = OffsetDateTime::parse(photo.taken_at.as_deref()?, &Rfc3339).ok()?;
    let position = photo.lat.zip(photo.lon);
    timezone::photo_offset(position, tz_name, taken_at).map(|offset| offset.whole_seconds())
}
