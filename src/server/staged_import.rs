//! The two-phase import (US-12): parse the GPX, suggest a name, then create
//! the trip once the owner has confirmed it.
//!
//! The single-step form in `import.rs` cannot satisfy US-12 — a suggested
//! `YYYY-mm-dd` prefix has to be *in the field* while the owner types, and
//! the date only exists once the track has been read. So the import screen
//! uploads the file first (`handle_stage_import`), fills in its second step
//! from what came back, and confirms (`handle_confirm_import`), which
//! promotes the parked parse into a trip.
//!
//! The file is therefore parsed once, not once per step: `derive_track` runs
//! at staging and its output waits in `import_staging` (`repo::staging`)
//! until confirmation copies it onto the `trip` and `track` rows through the
//! same `insert_trip_in_tx` every other import path uses.
//!
//! A sibling module rather than more of `import.rs`, the way `edit.rs` and
//! `delete.rs` already sit beside it — but every rule about what a field
//! *means* is imported from there, so the two entry points cannot drift on
//! how a blank name, activity type, kind or timezone is resolved.

use axum::{
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::config;
use crate::models::{ConfirmImport, ImportedTrip, StagedImport};
use crate::server::{
    error::AppError,
    gpx::TrackStats,
    import::{
        date_prefix, derive_track, read_gpx_field, resolve_activity_type, resolve_name,
        resolve_timezone, resolve_trip_kind,
    },
    repo::{self, insert_trip_in_tx, NewStagedImport, NewTrip},
    state::AppState,
};

/// What a parked parse holds, as JSON in `import_staging.derived`. The track
/// geometry and the original upload travel in their own columns, already in
/// the shape `NewTrip` wants; this is the rest of what `derive_track`
/// produced and confirmation still needs.
///
/// `timed_points` is deliberately absent: photos are attached after the trip
/// exists (ADR-0004's 2026-09-01 amendment), and that path recomputes them
/// from the stored GeoJSON.
#[derive(Serialize, Deserialize)]
struct StagedTrack {
    gpx_name: Option<String>,
    stats: TrackStats,
    guessed_tz: String,
}

/// `POST /api/import/staged` — phase one. Accepts a `multipart/form-data`
/// body with a `gpx` file field, parses it, and answers with what the confirm
/// step needs to fill itself in.
///
/// **Creates no trip.** An import abandoned at the naming step leaves the
/// archive exactly as it was, which is the reason the parse waits in a table
/// of its own rather than as a half-built trip row.
///
/// Errors match the one-shot import's, because they are the same file being
/// refused: 400 for a missing upload, 422 for GPX we cannot use.
pub async fn handle_stage_import(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let raw = read_gpx_field(multipart).await?;
    let derived = derive_track(&raw)?;

    // Abandoned parses only pile up when imports happen, so this is when it
    // is worth looking — no background task to own.
    let now = OffsetDateTime::now_utc();
    repo::sweep_staged_imports(&state.pool, now - config::server::STAGED_IMPORT_TTL).await?;

    let suggested_name = suggest_name(derived.name.as_deref(), derived.stats.start_time);
    let start_date = date_prefix(derived.stats.start_time);
    let staged = StagedTrack {
        gpx_name: derived.name,
        stats: derived.stats,
        guessed_tz: derived.guessed_tz,
    };
    let derived_json = serde_json::to_string(&staged)
        .map_err(|e| AppError::Internal(format!("could not park the parsed track: {e}")))?;

    let staging_id = repo::insert_staged_import(
        &state.pool,
        &NewStagedImport {
            derived: &derived_json,
            geojson: &derived.geojson,
            gpx: &raw,
        },
        now,
    )
    .await?;

    Ok(Json(StagedImport {
        staging_id,
        suggested_name,
        start_date,
        gpx_name: staged.gpx_name,
        timezone: staged.guessed_tz,
        distance_m: staged.stats.distance_m,
        ascent_m: staged.stats.ascent_m,
        duration_secs: staged.stats.duration_secs,
    }))
}

/// `POST /api/import/staged/:id/confirm` — phase two. Promotes the parked
/// parse into a trip carrying the owner's name, activity type (US-11), kind
/// (US-31) and timezone (US-4).
///
/// Reading the staged row, deleting it and inserting the trip all happen on
/// one transaction, which gives two properties worth having: a double submit
/// cannot import the same file twice, and a refused confirmation — an
/// activity type or kind the archive does not know — rolls the take back, so
/// the parse is still there for the owner's next try instead of costing them
/// the upload.
///
/// `201` with the new trip's id rather than a redirect: this is read by a
/// browser `fetch`, which cannot decline to follow one (US-42).
pub async fn handle_confirm_import(
    State(state): State<AppState>,
    Path(staging_id): Path<i64>,
    Json(confirm): Json<ConfirmImport>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = state.pool.begin().await?;

    let row = repo::take_staged_import_in_tx(&mut tx, staging_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let staged: StagedTrack = serde_json::from_str(&row.derived)
        .map_err(|e| AppError::Internal(format!("could not read the parked track: {e}")))?;

    let name = resolve_name(confirm.name, staged.gpx_name, staged.stats.start_time);
    let activity = resolve_activity_type(confirm.activity_type)?;
    let kind = resolve_trip_kind(confirm.kind)?;
    let tz_name = resolve_timezone(confirm.timezone, staged.guessed_tz)?;

    let trip_id = insert_trip_in_tx(
        &mut tx,
        &NewTrip {
            name: &name,
            activity_type: activity,
            tz_name: &tz_name,
            stats: &staged.stats,
            geojson: &row.geojson,
            gpx: &row.gpx,
            trip_kind: kind,
        },
    )
    .await?;
    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, format!("/app/trips/{trip_id}"))],
        Json(ImportedTrip { id: trip_id }),
    ))
}

/// `DELETE /api/import/staged/:id` — the owner left the screen, or picked a
/// different file. Dropping the parse now rather than leaving it to the
/// sweeper keeps the table as short-lived as the flow it serves.
pub async fn handle_cancel_staged_import(
    State(state): State<AppState>,
    Path(staging_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    match repo::delete_staged_import(&state.pool, staging_id).await? {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(AppError::NotFound),
    }
}

/// What to prefill the name field with (US-12).
///
/// The date prefix is the point of the story, so it leads whenever the track
/// has one: `"2024-06-01 Oslo Hills Walk"` with a track name to follow it,
/// and the bare `"2024-06-01 "` without — the owner types the rest after the
/// date rather than deleting a placeholder first. A GPX with no timestamps
/// has no date to offer, so its name (or nothing) stands alone.
///
/// This is a *suggestion*, not the fallback `resolve_name` applies when the
/// field arrives empty; that precedence is unchanged and still decides what
/// an unanswered confirm stores.
fn suggest_name(gpx_name: Option<&str>, start_time: Option<OffsetDateTime>) -> String {
    let name = gpx_name.map(str::trim).filter(|n| !n.is_empty());
    match (date_prefix(start_time), name) {
        (Some(prefix), Some(name)) => format!("{prefix} {name}"),
        (Some(prefix), None) => format!("{prefix} "),
        (None, Some(name)) => name.to_string(),
        (None, None) => String::new(),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::suggest_name;
    use time::macros::datetime;

    #[test]
    fn us12_a_named_track_is_suggested_behind_its_date() {
        assert_eq!(
            suggest_name(
                Some("Oslo Hills Walk"),
                Some(datetime!(2024-06-01 08:00 UTC))
            ),
            "2024-06-01 Oslo Hills Walk"
        );
    }

    #[test]
    fn us12_an_unnamed_track_suggests_the_bare_prefix_to_type_after() {
        assert_eq!(
            suggest_name(None, Some(datetime!(2024-06-01 08:00 UTC))),
            "2024-06-01 "
        );
    }

    #[test]
    fn us12_a_blank_track_name_counts_as_none() {
        assert_eq!(
            suggest_name(Some("   "), Some(datetime!(2024-06-01 08:00 UTC))),
            "2024-06-01 "
        );
    }

    #[test]
    fn us12_a_track_without_timestamps_offers_no_date_to_prefix() {
        // Not "Unknown date …": a prefill is something the owner keeps and
        // types after, and no one wants to delete that first.
        assert_eq!(
            suggest_name(Some("Oslo Hills Walk"), None),
            "Oslo Hills Walk"
        );
        assert_eq!(suggest_name(None, None), "");
    }
}
