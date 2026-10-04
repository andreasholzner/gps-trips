//! What the archive suggests for a trip as it is stored (US-74), for the
//! edit form to offer next to its fields.

use trip_archive_types::TripSuggestion;

use super::{get_json, ApiClient, ApiError};

/// `GET /api/trips/:id/suggestion` — the name the archive suggests for the
/// trip: its date, then the places its track passes.
pub async fn trip_suggestion(archive: &ApiClient, id: i64) -> Result<TripSuggestion, ApiError> {
    get_json(archive, archive.url(&format!("/api/trips/{id}/suggestion"))).await
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{import_sample, serve_test_archive};

    #[tokio::test]
    async fn us74_a_stored_trip_s_suggestion_comes_from_its_track() {
        // The test archive has no place database: the suggestion is the
        // date and the GPX name, whatever the trip is called now.
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[("name", "My walk")]).await;

        let suggestion = trip_suggestion(&archive, id).await.expect("suggestion");

        assert_eq!(suggestion.name, "2024-06-01 Oslo Hills Walk");
    }

    #[tokio::test]
    async fn us74_a_trip_that_does_not_exist_has_no_suggestion() {
        let (archive, _dir) = serve_test_archive().await;

        let error = trip_suggestion(&archive, 999).await.expect_err("a 404");

        assert!(error.is_not_found());
    }
}
