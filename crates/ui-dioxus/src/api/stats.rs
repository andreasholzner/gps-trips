//! What the statistics screen adds up (US-77).

use trip_archive_types::StatsTrips;

use super::{get_json, ApiClient, ApiError};

/// `GET /api/stats/trips` — every dated recorded trip, reduced to the
/// figures' ingredients.
pub async fn stats_trips(archive: &ApiClient) -> Result<StatsTrips, ApiError> {
    get_json(archive, archive.url("/api/stats/trips")).await
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{import_sample, serve_test_archive};

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
}
