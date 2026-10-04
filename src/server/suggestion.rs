//! What the archive suggests for a trip: its name behind its date (US-12,
//! US-74). Behind one interface that takes the trip rather than just its
//! track (ADR-0027), so what else is known about a trip can inform the
//! suggestion without its callers changing — the import screen's staging
//! step and the edit form's `GET /api/trips/:id/suggestion`.

use axum::{
    extract::{Path, State},
    Json,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::models::TripSuggestion;
use crate::server::{
    error::AppError,
    gpx::{parse_gpx, TrackPoint},
    import::date_prefix,
    name_suggestion::{lookup_boxes, place_part},
    places::PlaceDb,
    repo,
    state::AppState,
};

/// What the suggestion knows about a trip.
pub struct Trip<'a> {
    pub points: &'a [TrackPoint],
    /// The GPX track's own `<name>`, which the suggestion falls back to.
    pub gpx_name: Option<&'a str>,
    pub start_time: Option<OffsetDateTime>,
    /// The zone the date is read in.
    pub tz_name: &'a str,
}

/// The name suggested for `trip`: its date, then the places its track
/// passes — or, with no place found or no place database, its GPX name
/// (US-12's suggestion). A place database that fails is logged and falls
/// back the same way: a suggestion never fails an import.
pub async fn suggest_name(places: Option<&PlaceDb>, trip: &Trip<'_>) -> String {
    let named = match places {
        Some(db) => match places_of(db, trip.points).await {
            Ok(named) => named,
            Err(e) => {
                tracing::warn!("Could not look up the places a trip passes: {e}");
                None
            }
        },
        None => None,
    };
    with_date(
        named.as_deref().or(trip.gpx_name),
        trip.start_time,
        trip.tz_name,
    )
}

async fn places_of(db: &PlaceDb, points: &[TrackPoint]) -> Result<Option<String>, sqlx::Error> {
    let coords: Vec<geo::Coord> = points
        .iter()
        .map(|p| geo::Coord { x: p.lon, y: p.lat })
        .collect();
    let places = db.within(&lookup_boxes(&coords)).await?;
    Ok(place_part(points, places))
}

/// `name` behind the date of `start_time` where the track is (US-12): the
/// bare `"2024-06-01 "` without a name — the owner types the rest after the
/// date rather than deleting a placeholder first. A track with no
/// timestamps has no date to offer, so its name (or nothing) stands alone.
///
/// This is a *suggestion*, not the fallback `resolve_name` applies when the
/// field arrives empty; that precedence is unchanged and still decides what
/// an unanswered confirm stores.
fn with_date(name: Option<&str>, start_time: Option<OffsetDateTime>, tz_name: &str) -> String {
    let name = name.map(str::trim).filter(|n| !n.is_empty());
    match (date_prefix(start_time, tz_name), name) {
        (Some(prefix), Some(name)) => format!("{prefix} {name}"),
        (Some(prefix), None) => format!("{prefix} "),
        (None, Some(name)) => name.to_string(),
        (None, None) => String::new(),
    }
}

/// `GET /api/trips/:id/suggestion` — what the edit form offers for the trip
/// as it is stored (US-74), worked out from its original GPX when the form
/// opens. 404 for a trip that does not exist.
pub async fn handle_trip_suggestion(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<TripSuggestion>, AppError> {
    let trip = repo::get_trip(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let gpx = repo::get_original_gpx(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let track = parse_gpx(&gpx.bytes)
        .map_err(|e| AppError::Internal(format!("the stored GPX cannot be read: {e}")))?;
    let start_time = trip
        .start_time
        .as_deref()
        .and_then(|t| OffsetDateTime::parse(t, &Rfc3339).ok());
    let name = suggest_name(
        state.places.as_ref(),
        &Trip {
            points: &track.points,
            gpx_name: track.name.as_deref(),
            start_time,
            tz_name: trip.tz_name.as_deref().unwrap_or("UTC"),
        },
    )
    .await;
    Ok(Json(TripSuggestion { name }))
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::with_date;
    use time::macros::datetime;

    #[test]
    fn us12_a_named_track_is_suggested_behind_its_date() {
        assert_eq!(
            with_date(
                Some("Oslo Hills Walk"),
                Some(datetime!(2024-06-01 08:00 UTC)),
                "Europe/Oslo",
            ),
            "2024-06-01 Oslo Hills Walk"
        );
    }

    #[test]
    fn us12_an_unnamed_track_suggests_the_bare_prefix_to_type_after() {
        assert_eq!(
            with_date(None, Some(datetime!(2024-06-01 08:00 UTC)), "Europe/Oslo"),
            "2024-06-01 "
        );
    }

    #[test]
    fn us12_a_blank_track_name_counts_as_none() {
        assert_eq!(
            with_date(
                Some("   "),
                Some(datetime!(2024-06-01 08:00 UTC)),
                "Europe/Oslo"
            ),
            "2024-06-01 "
        );
    }

    #[test]
    fn us12_the_suggested_date_is_the_one_where_the_track_is() {
        // The field US-12 exists to prefill must not offer the wrong day for
        // a ride that started after midnight local time.
        assert_eq!(
            with_date(
                Some("Midnight Ride"),
                Some(datetime!(2024-06-01 22:30 UTC)),
                "Europe/Oslo",
            ),
            "2024-06-02 Midnight Ride"
        );
    }

    #[test]
    fn us12_a_track_without_timestamps_offers_no_date_to_prefix() {
        // Not "Unknown date …": a prefill is something the owner keeps and
        // types after, and no one wants to delete that first.
        assert_eq!(
            with_date(Some("Oslo Hills Walk"), None, "Europe/Oslo"),
            "Oslo Hills Walk"
        );
        assert_eq!(with_date(None, None, "Europe/Oslo"), "");
    }
}
