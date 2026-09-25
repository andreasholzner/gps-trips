//! US-53 — shares: read-only access to a few trips through a link.
//!
//! Two halves. The owner creates a share (`POST /api/shares`), lists the
//! active ones (`GET /api/shares`) and stops one (`DELETE /api/shares/:id`,
//! US-69), behind the gate like every owner route. The recipient reaches a route set of the
//! share's own under `/s/:token`, where the gate has already turned the token
//! into a [`Principal::Share`] — or answered 404 for one that opens nothing.
//! These routes are read-only by construction, and each checks that the trip
//! or photo it is asked for is one the share names: no owner handler is
//! reachable from here, so none of them needs to know shares exist.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    routing::get,
    Extension, Json, Router,
};
use time::OffsetDateTime;

use crate::config;
use crate::models::{
    ActiveShare, CreateShare, CreatedShare, PhotoResponse, Principal, ShareExpiry, ShareOverview,
    SharedTrip, TripDetail,
};
use crate::server::{
    error::AppError,
    http::{gpx_response, media_response, track_response},
    photo_api::respond_under,
    repo::{self, NewShare},
    state::AppState,
};

/// The recipient's routes, every one under `/s/:token` and every one a
/// `GET`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/s/:token/api/share", get(overview))
        .route("/s/:token/api/trips/:id", get(trip))
        .route("/s/:token/api/trips/:id/track.geojson", get(track))
        .route("/s/:token/api/trips/:id/photos", get(photos))
        .route("/s/:token/api/trips/:id/gpx", get(gpx))
        .route("/s/:token/media/*key", get(media))
}

// ── The owner's half ─────────────────────────────────────────────────────────

/// POST `/api/shares` — share a few trips. 400 for no trips or an overlong
/// label, 404 if a trip does not exist; 201 with the token otherwise.
pub async fn handle_create_share(
    State(state): State<AppState>,
    Json(body): Json<CreateShare>,
) -> Result<(StatusCode, Json<CreatedShare>), AppError> {
    if body.trip_ids.is_empty() {
        return Err(AppError::BadRequest("no trips selected".to_string()));
    }
    let label = body
        .label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty());
    if label.is_some_and(|label| label.chars().count() > config::share::LABEL_MAX_CHARS) {
        return Err(AppError::BadRequest(format!(
            "the label is longer than {} characters",
            config::share::LABEL_MAX_CHARS
        )));
    }
    if !repo::trips_exist(&state.pool, &body.trip_ids).await? {
        return Err(AppError::NotFound);
    }

    let now = OffsetDateTime::now_utc();
    let expires_at = expiry_at(body.expiry, now);
    let token = mint_token()?;
    repo::insert_share(
        &state.pool,
        &NewShare {
            token: &token,
            label,
            trip_ids: &body.trip_ids,
            created_at: now,
            expires_at,
        },
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedShare {
            token,
            expires_at: expires_at.map(repo::to_rfc3339),
        }),
    ))
}

/// GET `/api/shares` — every share that still opens something, newest
/// first (US-69).
pub async fn handle_list_shares(
    State(state): State<AppState>,
) -> Result<Json<Vec<ActiveShare>>, AppError> {
    Ok(Json(
        repo::list_active_shares(&state.pool, OffsetDateTime::now_utc()).await?,
    ))
}

/// DELETE `/api/shares/:id` — stop a share (US-69): its link answers as an
/// unknown one from the next request on. 204; 404 for a share that is not
/// active, so stopping one twice is not a success.
pub async fn handle_stop_share(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    if repo::stop_share(&state.pool, id, OffsetDateTime::now_utc()).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

/// When a share chosen at `now` stops working.
fn expiry_at(expiry: ShareExpiry, now: OffsetDateTime) -> Option<OffsetDateTime> {
    match expiry {
        ShareExpiry::Never => None,
        ShareExpiry::OneMonth => Some(now + config::share::ONE_MONTH),
        ShareExpiry::SixMonths => Some(now + config::share::SIX_MONTHS),
    }
}

/// A fresh token: [`config::share::TOKEN_BYTES`] from the OS's generator,
/// as hex so it sits in a path without escaping.
fn mint_token() -> Result<String, AppError> {
    let mut bytes = [0u8; config::share::TOKEN_BYTES];
    getrandom::fill(&mut bytes)
        .map_err(|err| AppError::Internal(format!("no randomness for a share token: {err}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

// ── The recipient's half ─────────────────────────────────────────────────────

/// The share the gate resolved. Anything else cannot reach these routes —
/// the gate puts a share principal on every `/s/` path or answers itself —
/// so this is a second lock on the same door, not the first.
fn share_id(principal: Principal) -> Result<i64, AppError> {
    match principal {
        Principal::Share { share_id } => Ok(share_id),
        Principal::Owner | Principal::Anonymous => Err(AppError::NotFound),
    }
}

/// `trip_id`, if the share names it; the same 404 as a trip that does not
/// exist otherwise.
async fn covered(state: &AppState, principal: Principal, trip_id: i64) -> Result<i64, AppError> {
    if repo::share_covers_trip(&state.pool, share_id(principal)?, trip_id).await? {
        Ok(trip_id)
    } else {
        Err(AppError::NotFound)
    }
}

/// GET `/s/:token/api/share` — the share's title and its trips.
async fn overview(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ShareOverview>, AppError> {
    let share_id = share_id(principal)?;
    Ok(Json(ShareOverview {
        label: repo::share_label(&state.pool, share_id).await?,
        trips: repo::list_shared_trips(&state.pool, share_id).await?,
    }))
}

/// GET `/s/:token/api/trips/:id` — one shared trip.
async fn trip(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((_, id)): Path<(String, i64)>,
) -> Result<Json<SharedTrip>, AppError> {
    let id = covered(&state, principal, id).await?;
    let trip = repo::get_trip(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(shared_trip(trip)))
}

/// GET `/s/:token/api/trips/:id/track.geojson`.
async fn track(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((_, id)): Path<(String, i64)>,
) -> Result<Response, AppError> {
    let id = covered(&state, principal, id).await?;
    track_response(&state, id).await
}

/// GET `/s/:token/api/trips/:id/gpx`.
async fn gpx(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((_, id)): Path<(String, i64)>,
) -> Result<Response, AppError> {
    let id = covered(&state, principal, id).await?;
    gpx_response(&state, id).await
}

/// GET `/s/:token/api/trips/:id/photos` — the trip's photos, addressed
/// through the share.
async fn photos(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((token, id)): Path<(String, i64)>,
) -> Result<Json<Vec<PhotoResponse>>, AppError> {
    let id = covered(&state, principal, id).await?;
    let trip = repo::get_trip(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let prefix = format!("{}{token}", config::share::PATH_PREFIX);
    Ok(Json(
        repo::list_photos(&state.pool, id)
            .await?
            .into_iter()
            .map(|photo| respond_under(&state, &prefix, photo, trip.tz_name.as_deref()))
            .collect(),
    ))
}

/// GET `/s/:token/media/*key` — a photo of a shared trip. Only a key the
/// database lists for one is served, never one read off the path's shape.
async fn media(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Path((_, key)): Path<(String, String)>,
) -> Result<Response, AppError> {
    if !repo::share_covers_blob(&state.pool, share_id(principal)?, &key).await? {
        return Err(AppError::NotFound);
    }
    media_response(&state, key).await
}

/// The recipient's copy of a trip, field by field — so a field the owner's
/// type grows later reaches a share only by being added here.
fn shared_trip(trip: TripDetail) -> SharedTrip {
    SharedTrip {
        id: trip.id,
        name: trip.name,
        activity_type: trip.activity_type,
        tz_name: trip.tz_name,
        start_time: trip.start_time,
        start_date: trip.start_date,
        end_time: trip.end_time,
        distance_m: trip.distance_m,
        ascent_m: trip.ascent_m,
        descent_m: trip.descent_m,
        duration_secs: trip.duration_secs,
        min_lat: trip.min_lat,
        min_lon: trip.min_lon,
        max_lat: trip.max_lat,
        max_lon: trip.max_lon,
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us53_an_expiry_is_counted_from_when_the_share_was_made() {
        let now = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        assert_eq!(expiry_at(ShareExpiry::Never, now), None);
        assert_eq!(
            expiry_at(ShareExpiry::OneMonth, now),
            Some(now + config::share::ONE_MONTH)
        );
        assert_eq!(
            expiry_at(ShareExpiry::SixMonths, now),
            Some(now + config::share::SIX_MONTHS)
        );
    }

    #[test]
    fn us53_a_token_is_long_random_and_fits_a_path() {
        let first = mint_token().unwrap();
        let second = mint_token().unwrap();
        assert_eq!(first.len(), config::share::TOKEN_BYTES * 2);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()), "{first}");
        assert_ne!(first, second);
    }

    #[test]
    fn us53_only_a_share_principal_names_a_share() {
        assert_eq!(share_id(Principal::Share { share_id: 7 }).unwrap(), 7);
        assert!(share_id(Principal::Owner).is_err());
        assert!(share_id(Principal::Anonymous).is_err());
    }
}
