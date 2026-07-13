use axum::{
    extract::{multipart::Field, Multipart, Path, State},
    response::{IntoResponse, Redirect},
};
use time::OffsetDateTime;

use crate::models::{ActivityType, TripDetail};
use crate::server::{
    error::{AppError, ImportError},
    geojson::{self, build_track_geojson},
    gpx::{self, compute_stats, parse_gpx, TimedPoint, TrackStats},
    photos::{ingest_photos, UploadedPhoto},
    placement::TripPhotoContext,
    repo::{self, insert_trip_in_tx},
    state::AppState,
    timezone,
};
use sqlx::SqlitePool;

/// Everything derivable from a GPX byte string that both import entry
/// points need: `handle_import` (this module) and Komoot sync
/// (`komoot_sync::sync_one_tour`, US-22). Kept as one function so the two
/// pipelines can't drift on how GPX bytes become a trip's stats/GeoJSON/
/// timezone guess/photo-placement timeline (ADR-0021: Komoot sync reuses
/// "the exact same pipeline" `handle_import` uses).
pub(crate) struct DerivedTrack {
    /// The GPX track's own `<name>`, if any — only meaningful to
    /// `handle_import`'s name-resolution precedence (US-12); Komoot sync
    /// always uses the tour's Komoot name instead.
    pub name: Option<String>,
    pub stats: TrackStats,
    pub geojson: String,
    pub guessed_tz: String,
    pub timed_points: Vec<TimedPoint>,
}

pub(crate) fn derive_track(raw: &[u8]) -> Result<DerivedTrack, ImportError> {
    let parsed = parse_gpx(raw)?;
    let stats = compute_stats(&parsed.points);
    let geojson = build_track_geojson(&parsed.points);
    let guessed_tz = timezone::guess_timezone_from_track(&parsed.points);
    let timed_points = gpx::timed_points(&parsed.points);
    Ok(DerivedTrack {
        name: parsed.name,
        stats,
        geojson,
        guessed_tz,
        timed_points,
    })
}

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
    let mut form_timezone: Option<String> = None;
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
            Some("timezone") => form_timezone = Some(read_text(field).await?),
            Some("photos") | Some("photo") => {
                if let Some(photo) = read_photo_field(field).await? {
                    photos.push(photo);
                }
            }
            _ => {} // ignore unknown fields
        }
    }

    let raw = gpx_bytes.ok_or_else(|| AppError::BadRequest("Missing 'gpx' field".to_string()))?;

    let derived = derive_track(&raw)?;

    let name = resolve_name(form_name, derived.name, derived.stats.start_time);
    let activity = resolve_activity_type(form_activity)?;
    let tz_name = resolve_timezone(form_timezone, derived.guessed_tz)?;

    // Trip, track and photos commit in one transaction, so a failed import
    // leaves no trip behind (reliability NFR; ADR-0004).
    let mut tx = state.pool.begin().await?;
    let trip_id = insert_trip_in_tx(
        &mut tx,
        &name,
        activity,
        &tz_name,
        &derived.stats,
        &derived.geojson,
        &raw,
    )
    .await?;
    let ctx = TripPhotoContext {
        timed_points: &derived.timed_points,
        tz_name: Some(&tz_name),
    };
    ingest_photos(&mut tx, &state.store, trip_id, &ctx, photos).await?;
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
    let trip = repo::get_trip(&state.pool, trip_id)
        .await?
        .ok_or(AppError::NotFound)?;

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

    let (timed_points, tz_name) = resolve_photo_context(&state.pool, trip_id, trip).await?;

    let ctx = TripPhotoContext {
        timed_points: &timed_points,
        tz_name: Some(&tz_name),
    };
    let mut tx = state.pool.begin().await?;
    ingest_photos(&mut tx, &state.store, trip_id, &ctx, photos).await?;
    tx.commit().await?;

    Ok(Redirect::to(&format!("/trips/{trip_id}")))
}

/// Resolve the track's timed points and a concrete timezone for adding photos
/// to an existing trip (US-4). Reads the trip's already-parsed GeoJSON
/// (`repo::get_track_geojson`) rather than re-parsing the original GPX XML —
/// this endpoint doesn't have the track in memory the way `handle_import`
/// does, and the stored GeoJSON already carries the same coordinate/timestamp
/// data in parsed form.
///
/// Self-healing: a trip imported before `tz_name` existed gets it computed
/// here (from the track's first point) and persisted, so it's stable and
/// concrete from then on.
async fn resolve_photo_context(
    pool: &SqlitePool,
    trip_id: i64,
    trip: TripDetail,
) -> Result<(Vec<TimedPoint>, String), AppError> {
    let timed_points = match repo::get_track_geojson(pool, trip_id).await? {
        Some(geojson) => geojson::parse_timed_points(&geojson),
        None => Vec::new(),
    };

    let tz_name = match trip.tz_name {
        Some(name) => name,
        None => {
            let guessed = timezone::guess_timezone_from_timed_points(&timed_points);
            repo::set_trip_timezone(pool, trip_id, &guessed).await?;
            guessed
        }
    };

    Ok((timed_points, tz_name))
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
        known_location: None,
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

/// Resolve an `activity_type` field into an `ActivityType` (ADR-0018): a
/// blank or missing field is `Unknown`; any other value (trimmed) must be one
/// of the known variants, or the request is rejected as a 400.
///
/// Shared by two callers with different meanings for "blank": import's
/// multipart form (blank/missing = the owner didn't specify one at creation
/// time) and `edit::handle_edit_trip`'s JSON `PATCH` body (an explicit blank
/// string = the owner deliberately resetting it back to unspecified; a
/// wholly-omitted JSON field is handled by the caller before this function
/// ever sees it, and leaves the trip's activity type untouched). Both callers
/// only ever submit a known value or blank through their respective UIs, so
/// an unrecognized non-blank value means a malformed/hand-crafted request,
/// not a case to silently paper over.
pub(crate) fn resolve_activity_type(
    form_activity: Option<String>,
) -> Result<ActivityType, AppError> {
    match form_activity.as_deref().map(str::trim) {
        None | Some("") => Ok(ActivityType::Unknown),
        Some(trimmed) => trimmed.parse().map_err(AppError::BadRequest),
    }
}

/// Resolve the `timezone` form field into a concrete IANA timezone (US-4,
/// ADR-0009/0019): a blank or missing field uses `guessed` (auto-detected
/// from the track's start coordinate); an explicit value must be a
/// recognized IANA name, or the request is rejected as a 400.
fn resolve_timezone(form_timezone: Option<String>, guessed: String) -> Result<String, AppError> {
    match form_timezone.filter(|t| !t.trim().is_empty()) {
        None => Ok(guessed),
        Some(value) if timezone::is_known_timezone(&value) => Ok(value),
        Some(value) => Err(AppError::BadRequest(format!("Unknown timezone: {value:?}"))),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{resolve_activity_type, resolve_name, resolve_timezone};
    use crate::models::ActivityType;
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

    // ADR-0018: activity_type as a closed enum, not a free-form string.

    #[test]
    fn resolve_activity_type_defaults_to_unknown_when_missing() {
        assert_eq!(resolve_activity_type(None).unwrap(), ActivityType::Unknown);
    }

    #[test]
    fn resolve_activity_type_defaults_to_unknown_when_blank() {
        assert_eq!(
            resolve_activity_type(Some("   ".to_string())).unwrap(),
            ActivityType::Unknown
        );
    }

    #[test]
    fn resolve_activity_type_parses_a_known_value() {
        assert_eq!(
            resolve_activity_type(Some("mountaineering".to_string())).unwrap(),
            ActivityType::Mountaineering
        );
    }

    #[test]
    fn resolve_activity_type_trims_surrounding_whitespace_before_parsing() {
        assert_eq!(
            resolve_activity_type(Some("  cycling  ".to_string())).unwrap(),
            ActivityType::Cycling
        );
    }

    #[test]
    fn resolve_activity_type_rejects_an_unrecognized_value() {
        assert!(resolve_activity_type(Some("unicycling".to_string())).is_err());
    }

    // US-4: the trip's timezone assumption for photo-timestamp interpolation.

    #[test]
    fn resolve_timezone_uses_the_guess_when_missing() {
        assert_eq!(
            resolve_timezone(None, "Europe/Oslo".to_string()).unwrap(),
            "Europe/Oslo"
        );
    }

    #[test]
    fn resolve_timezone_uses_the_guess_when_blank() {
        assert_eq!(
            resolve_timezone(Some("   ".to_string()), "Europe/Oslo".to_string()).unwrap(),
            "Europe/Oslo"
        );
    }

    #[test]
    fn resolve_timezone_accepts_a_recognized_override() {
        assert_eq!(
            resolve_timezone(Some("Europe/Paris".to_string()), "Europe/Oslo".to_string()).unwrap(),
            "Europe/Paris"
        );
    }

    #[test]
    fn resolve_timezone_rejects_an_unrecognized_override() {
        assert!(
            resolve_timezone(Some("Not/A_Zone".to_string()), "Europe/Oslo".to_string()).is_err()
        );
    }
}
