//! The Tags screen's calls (US-83): every tag with its trips and shares,
//! creating a tag, and deleting one.

use serde::Serialize;
use trip_archive_types::{Tag, TagOverview};

use super::{get_json, ok_or_error, ApiClient, ApiError};

/// `GET /api/tags/overview` — every tag, alphabetical, with how many trips
/// carry it and the active summary shares naming it.
pub async fn tag_overview(archive: &ApiClient) -> Result<Vec<TagOverview>, ApiError> {
    get_json(archive, archive.url("/api/tags/overview")).await
}

/// The `POST /api/tags` body: the name as typed. The archive normalizes it
/// as it does a trip's tag (US-33).
#[derive(Serialize)]
struct CreateTag<'a> {
    name: &'a str,
}

/// `POST /api/tags` — create a tag carrying no trips. A name the archive
/// refuses, or one that exists already, comes back in the archive's own
/// words, so the owner reads why.
pub async fn create_tag(archive: &ApiClient, name: &str) -> Result<Tag, ApiError> {
    let url = archive.url("/api/tags");
    let response = archive
        .post(&url)
        .json(&CreateTag { name })
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response)
        .await?
        .json()
        .await
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

/// `DELETE /api/tags/:id` — delete a tag: it comes off every trip, and the
/// summary shares naming it are narrowed or stopped.
pub async fn delete_tag(archive: &ApiClient, id: i64) -> Result<(), ApiError> {
    let url = archive.url(&format!("/api/tags/{id}"));
    let response = archive
        .delete(&url)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response).await?;
    Ok(())
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{import_sample, serve_test_archive, tag_trip};

    #[tokio::test]
    async fn us83_a_created_tag_is_listed_and_a_deleted_one_is_gone() {
        let (archive, _dir) = serve_test_archive().await;
        let trip = import_sample(&archive, &[]).await;
        tag_trip(&archive, trip, "norway").await;

        let alps = create_tag(&archive, " Alps").await.expect("create");
        assert_eq!(alps.name, "alps");
        let listed = tag_overview(&archive).await.expect("overview");
        let rows: Vec<(&str, i64)> = listed
            .iter()
            .map(|tag| (tag.name.as_str(), tag.trip_count))
            .collect();
        assert_eq!(rows, vec![("alps", 0), ("norway", 1)]);

        delete_tag(&archive, alps.id).await.expect("delete");
        let listed = tag_overview(&archive).await.expect("overview");
        assert_eq!(listed.len(), 1);
    }

    #[tokio::test]
    async fn us83_a_refused_name_comes_back_in_the_archives_words() {
        let (archive, _dir) = serve_test_archive().await;
        create_tag(&archive, "alps").await.expect("create");

        let existing = create_tag(&archive, "ALPS").await.unwrap_err();
        let invalid = create_tag(&archive, "day,trip").await.unwrap_err();

        assert_eq!(existing.to_string(), "tag \"alps\" already exists");
        assert_eq!(invalid.to_string(), "tag name cannot contain a comma");
    }
}
