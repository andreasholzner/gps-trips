//! What the statistics screen (US-77) and the tag summary (US-78) add up.
//! Kept out of `http.rs`, as `tags.rs` and `tracks.rs` keep their own
//! concerns. Both answer the trips reduced to the figures' ingredients; the
//! figures themselves are the SPA's to work out.

use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

use crate::models::{StatsTrips, TagSummaries};
use crate::server::{error::AppError, filter::parse_tags, repo, state::AppState};

/// GET `/api/stats/trips` — every dated recorded trip, reduced to what the
/// statistics screen adds up (US-77).
pub async fn handle_stats_trips(
    State(state): State<AppState>,
) -> Result<Json<StatsTrips>, AppError> {
    let (trips, undated) = repo::list_stats_trips(&state.pool).await?;
    Ok(Json(StatsTrips {
        trips,
        undated,
        today: time::OffsetDateTime::now_utc().date().to_string(),
    }))
}

/// The `GET /api/stats/tags` query: `tags=alps,norway`, as the trip list's
/// tag filter takes it (US-38).
#[derive(Deserialize)]
pub struct TagsQuery {
    tags: Option<String>,
}

/// GET `/api/stats/tags?tags=…` — the dated recorded trips under any of the
/// chosen tags, and which tag holds which (US-78). No tags ask for nothing;
/// 400 for a malformed tag name, while an unknown one just has no trips.
pub async fn handle_tag_summaries(
    State(state): State<AppState>,
    Query(query): Query<TagsQuery>,
) -> Result<Json<TagSummaries>, AppError> {
    let tags = parse_tags(query.tags.as_deref())?;
    let (tags, trips) = repo::list_tag_summaries(&state.pool, &tags).await?;
    Ok(Json(TagSummaries { tags, trips }))
}
