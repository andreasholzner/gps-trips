//! One waypoint of a collection (US-98): read for its page, corrected and
//! deleted over the JSON API.

use trip_archive_types::{WaypointDetail, WaypointEdit};

use super::{get_json, ok_or_error, ApiClient, ApiError};

/// `GET /api/waypoints/:id` — the waypoint, its collection and its track. A
/// 404 travels back as such.
pub async fn get_waypoint(archive: &ApiClient, id: i64) -> Result<WaypointDetail, ApiError> {
    get_json(archive, archive.url(&format!("/api/waypoints/{id}"))).await
}

/// `PATCH /api/waypoints/:id` — correct the waypoint; a refusal comes back
/// in the archive's words.
pub async fn edit_waypoint(
    archive: &ApiClient,
    id: i64,
    edit: &WaypointEdit,
) -> Result<(), ApiError> {
    let url = archive.url(&format!("/api/waypoints/{id}"));
    let response = archive
        .patch(&url)
        .json(edit)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response).await?;
    Ok(())
}

/// `DELETE /api/waypoints/:id` — the waypoint alone; its collection and
/// tracks stay.
pub async fn delete_waypoint(archive: &ApiClient, id: i64) -> Result<(), ApiError> {
    let url = archive.url(&format!("/api/waypoints/{id}"));
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
    use crate::api::list_collection_waypoints;
    use crate::test_support::{import_collection, serve_test_archive, COLLECTION_GPX};

    #[tokio::test]
    async fn us98_a_waypoint_is_read_corrected_and_deleted() {
        let (archive, _dir) = serve_test_archive().await;
        let (collection, _) = import_collection(&archive, COLLECTION_GPX, &[]).await;
        let id = list_collection_waypoints(&archive, collection)
            .await
            .unwrap()[0]
            .id;

        let read = get_waypoint(&archive, id).await.unwrap();
        assert_eq!(read.collection_id, collection);

        let edit = WaypointEdit {
            name: Some("Passo".to_string()),
            ..Default::default()
        };
        edit_waypoint(&archive, id, &edit).await.unwrap();
        let read = get_waypoint(&archive, id).await.unwrap();
        assert_eq!(read.waypoint.name.as_deref(), Some("Passo"));

        let refused = WaypointEdit {
            name: Some(" ".to_string()),
            ..Default::default()
        };
        let err = edit_waypoint(&archive, id, &refused).await.unwrap_err();
        assert!(err.to_string().contains("name"), "{err}");

        delete_waypoint(&archive, id).await.unwrap();
        assert!(get_waypoint(&archive, id).await.unwrap_err().is_not_found());
    }
}
