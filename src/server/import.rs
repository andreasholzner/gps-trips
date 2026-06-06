use axum::{
    extract::{multipart::Field, Multipart, Path, State},
    response::{IntoResponse, Redirect},
};
use time::OffsetDateTime;

use crate::server::{
    error::AppError,
    geojson::build_track_geojson,
    gpx::{compute_stats, parse_gpx},
    photos::{ingest_photos, UploadedPhoto},
    repo::{self, insert_trip_in_tx},
    state::AppState,
};

/// `POST /api/import` — accepts a `multipart/form-data` body with a required
/// `gpx` file field, optional `name` and `activity_type` text fields, and any
/// number of `photos` file fields (US-2: photos uploaded with the import).
///
/// On success: stores the trip, its track and its photos in one transaction and
/// redirects to `/trips/{id}` (303 See Other). On failure: a 4xx with a
/// plain-text message — 400 for a malformed/missing upload, 422 for GPX content
/// we cannot use.
///
/// Plain Axum handler, not a Leptos server function (ADR-0004).
pub async fn handle_import(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let mut gpx_bytes: Option<Vec<u8>> = None;
    let mut form_name: Option<String> = None;
    let mut form_activity: Option<String> = None;
    let mut photos: Vec<UploadedPhoto> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        match field.name() {
            Some("gpx") => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                gpx_bytes = Some(bytes.to_vec());
            }
            Some("name") => form_name = Some(read_text(field).await?),
            Some("activity_type") => form_activity = Some(read_text(field).await?),
            Some("photos") | Some("photo") => {
                if let Some(photo) = read_photo_field(field).await? {
                    photos.push(photo);
                }
            }
            _ => {} // ignore unknown fields
        }
    }

    let raw = gpx_bytes.ok_or_else(|| AppError::BadRequest("Missing 'gpx' field".to_string()))?;

    let parsed = parse_gpx(&raw)?;
    let stats = compute_stats(&parsed.points);
    let geojson = build_track_geojson(&parsed.points);

    let name = resolve_name(form_name, parsed.name, stats.start_time);
    let activity = form_activity
        .filter(|a| !a.trim().is_empty())
        .unwrap_or_else(|| "unspecified".to_string());

    // Trip, track and photos commit in one transaction, so a failed import
    // leaves no trip behind (reliability NFR; ADR-0004).
    let mut tx = state.pool.begin().await?;
    let trip_id = insert_trip_in_tx(&mut tx, &name, &activity, &stats, &geojson, &raw).await?;
    ingest_photos(&mut tx, &state.store, trip_id, photos).await?;
    tx.commit().await?;

    Ok(Redirect::to(&format!("/trips/{trip_id}")))
}

/// `POST /api/trips/:id/photos` — attach photos to an existing trip (US-2:
/// photos can be added at a later time). Reuses the exact import ingestion path.
/// Redirects back to the trip detail page; 404 if the trip does not exist.
pub async fn handle_add_photos(
    State(state): State<AppState>,
    Path(trip_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    if repo::get_trip(&state.pool, trip_id).await?.is_none() {
        return Err(AppError::NotFound);
    }

    let mut photos: Vec<UploadedPhoto> = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        if matches!(field.name(), Some("photos") | Some("photo")) {
            if let Some(photo) = read_photo_field(field).await? {
                photos.push(photo);
            }
        }
    }

    let mut tx = state.pool.begin().await?;
    ingest_photos(&mut tx, &state.store, trip_id, photos).await?;
    tx.commit().await?;

    Ok(Redirect::to(&format!("/trips/{trip_id}")))
}

async fn read_text(field: Field<'_>) -> Result<String, AppError> {
    field
        .text()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))
}

/// Read one `photos` field into an `UploadedPhoto`, or `None` for an empty part
/// (browsers send an empty file part when no file was chosen).
async fn read_photo_field(field: Field<'_>) -> Result<Option<UploadedPhoto>, AppError> {
    let original_name = field.file_name().map(|s| s.to_string());
    let content_type = field.content_type().map(|s| s.to_string());
    let bytes = field
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(UploadedPhoto {
        original_name: original_name.unwrap_or_else(|| "photo".to_string()),
        content_type,
        bytes: bytes.to_vec(),
    }))
}

/// Choose the trip name (US-12): an explicit form value wins; otherwise fall back
/// to the GPX track name; otherwise a `YYYY-MM-DD`-prefixed default derived from
/// the track's start time.
fn resolve_name(
    form_name: Option<String>,
    gpx_name: Option<String>,
    start_time: Option<OffsetDateTime>,
) -> String {
    if let Some(name) = form_name.filter(|n| !n.trim().is_empty()) {
        return name;
    }
    if let Some(name) = gpx_name.filter(|n| !n.trim().is_empty()) {
        return name;
    }
    let prefix = start_time
        .map(|t| {
            let d = t.date();
            format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day())
        })
        .unwrap_or_else(|| "Unknown date".to_string());
    format!("{prefix} Imported Trip")
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::resolve_name;
    use time::macros::datetime;

    // US-12: trip naming precedence and the date-prefixed default.

    #[test]
    fn us12_explicit_form_name_takes_precedence() {
        let name = resolve_name(
            Some("Morning Ride".to_string()),
            Some("GPX Track Name".to_string()),
            Some(datetime!(2024-06-01 08:00 UTC)),
        );
        assert_eq!(name, "Morning Ride");
    }

    #[test]
    fn us12_blank_form_name_falls_back_to_gpx_track_name() {
        let name = resolve_name(
            Some("   ".to_string()),
            Some("GPX Track Name".to_string()),
            None,
        );
        assert_eq!(name, "GPX Track Name");
    }

    #[test]
    fn us12_without_any_name_uses_date_prefixed_default() {
        let name = resolve_name(None, None, Some(datetime!(2024-06-01 08:00 UTC)));
        assert_eq!(name, "2024-06-01 Imported Trip");
    }

    #[test]
    fn us12_without_name_or_start_time_uses_unknown_date() {
        let name = resolve_name(None, None, None);
        assert_eq!(name, "Unknown date Imported Trip");
    }
}
