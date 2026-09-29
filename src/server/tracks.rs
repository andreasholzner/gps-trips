//! Many trips' tracks in one request (US-73): the trip-list map draws the
//! trips in view as lines, and fetching each trip's `track.geojson` in turn
//! made a dense view slow. Kept out of `http.rs`, as `tags.rs` and
//! `edit.rs` keep their own concerns.

use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

use crate::models::TripTrack;
use crate::server::{error::AppError, repo, state::AppState};

/// The `GET /api/trips/tracks` query: `ids=1,2,3`. A share's route takes
/// the same.
#[derive(Deserialize)]
pub struct TracksQuery {
    ids: Option<String>,
}

impl TracksQuery {
    /// The `ids` parameter as given, empty when it is missing.
    pub fn ids(&self) -> &str {
        self.ids.as_deref().unwrap_or("")
    }
}

/// GET `/api/trips/tracks?ids=…` — each requested trip's stored positions,
/// as `[lon, lat]` (US-73). A trip that does not exist, or has no track, is
/// left out; no ids ask for nothing; 400 for ids that are not a
/// comma-separated list of integers.
pub async fn handle_list_tracks(
    State(state): State<AppState>,
    Query(query): Query<TracksQuery>,
) -> Result<Json<Vec<TripTrack>>, AppError> {
    let ids = parse_ids(query.ids())?;
    if ids.is_empty() {
        return Ok(Json(Vec::new()));
    }
    Ok(Json(repo::list_track_positions(&state.pool, &ids).await?))
}

/// The `ids` parameter as trip ids. Empty for an empty parameter; an empty
/// part between commas is malformed rather than skipped.
pub fn parse_ids(param: &str) -> Result<Vec<i64>, AppError> {
    if param.trim().is_empty() {
        return Ok(Vec::new());
    }
    param
        .split(',')
        .map(|part| {
            part.trim().parse().map_err(|_| {
                AppError::BadRequest(format!(
                    "ids must be trip ids separated by commas, not {part:?}"
                ))
            })
        })
        .collect()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_read_off_a_comma_separated_list() {
        assert_eq!(parse_ids("1,22, 333").unwrap(), [1, 22, 333]);
    }

    #[test]
    fn an_empty_parameter_asks_for_nothing() {
        assert!(parse_ids("").unwrap().is_empty());
        assert!(parse_ids("  ").unwrap().is_empty());
    }

    #[test]
    fn anything_but_integers_is_refused() {
        for param in ["1,x", "1,,2", "1.5", ",1", "1,"] {
            assert!(parse_ids(param).is_err(), "{param}");
        }
    }
}
