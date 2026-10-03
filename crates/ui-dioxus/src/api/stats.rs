//! What the statistics screen (US-77) and the tag summary (US-78) add up.

use trip_archive_types::{StatsTrips, TagSummaries};

use super::{get_json, ApiClient, ApiError};
use crate::filters::encode;

/// `GET /api/stats/trips` — every dated recorded trip, reduced to the
/// figures' ingredients.
pub async fn stats_trips(archive: &ApiClient) -> Result<StatsTrips, ApiError> {
    get_json(archive, archive.url("/api/stats/trips")).await
}

/// `GET /api/stats/tags?tags=…` — the dated recorded trips under any of
/// `tags`, and which tag holds which. No tags ask for nothing, without a
/// request.
pub async fn tag_summaries(archive: &ApiClient, tags: &[String]) -> Result<TagSummaries, ApiError> {
    if tags.is_empty() {
        return Ok(TagSummaries {
            tags: Vec::new(),
            trips: Vec::new(),
        });
    }
    let tags = encode(&tags.join(","));
    get_json(
        archive,
        archive.url(&format!("/api/stats/tags?tags={tags}")),
    )
    .await
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{import_sample, serve_test_archive, tag_trip};

    #[tokio::test]
    async fn us77_the_recorded_trips_come_with_their_local_dates_and_moving_time() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[("activity_type", "hiking")]).await;
        import_sample(&archive, &[("kind", "planned")]).await;

        let stats = stats_trips(&archive).await.expect("stats");

        assert_eq!(stats.trips.len(), 1);
        assert_eq!(stats.trips[0].id, id);
        assert_eq!(stats.trips[0].start_date, "2024-06-01");
        assert_eq!(stats.trips[0].moving_secs, Some(3600));
        assert_eq!(stats.undated, 0);
    }

    #[tokio::test]
    async fn us78_the_chosen_tags_come_with_their_trips() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;
        tag_trip(&archive, id, "alps&co").await;

        let tags = vec!["alps&co".to_string(), "norway".to_string()];
        let summary = tag_summaries(&archive, &tags).await.expect("summary");

        assert_eq!(summary.tags.len(), 2);
        assert_eq!(summary.tags[0].name, "alps&co");
        assert_eq!(summary.tags[0].trip_ids, [id]);
        assert!(summary.tags[1].trip_ids.is_empty());
        assert_eq!(summary.trips.len(), 1);
        assert_eq!(summary.trips[0].id, id);
    }
}
