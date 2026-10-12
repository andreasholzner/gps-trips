//! Adding files to a collection, or making one of several (US-88): the
//! upload parked for review, the review again with the owner's decisions,
//! its confirmation or cancelling, and the files a collection keeps.

use trip_archive_types::{
    CollectionFile, CollectionImportReview, ConfirmCollectionImport, CreatedCollection,
    ImportDecisions,
};

use super::super::{get_json, ok_or_error, ApiClient, ApiError};
use super::gpx_part;

/// `POST /api/collection-imports` — park `files` for review, to add them to
/// `collection_id`, or to make a new collection of them. A file the archive
/// cannot use comes back refused, named, in its words.
pub async fn upload_collection_files(
    archive: &ApiClient,
    files: Vec<(String, Vec<u8>)>,
    collection_id: Option<i64>,
) -> Result<CollectionImportReview, ApiError> {
    let url = archive.url("/api/collection-imports");
    let mut form = reqwest::multipart::Form::new();
    for (file_name, bytes) in files {
        form = form.part("gpx", gpx_part(&file_name, bytes)?);
    }
    if let Some(id) = collection_id {
        form = form.text("collection_id", id.to_string());
    }
    let response = archive
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response)
        .await?
        .json()
        .await
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

/// `POST /api/collection-imports/:id/review` — the proposals again, with the
/// owner's decisions so far. A 404 — spent, swept or cancelled — travels
/// back as such.
pub async fn review_collection_import(
    archive: &ApiClient,
    staging_id: i64,
    decisions: &ImportDecisions,
) -> Result<CollectionImportReview, ApiError> {
    let url = archive.url(&format!("/api/collection-imports/{staging_id}/review"));
    let response = archive
        .post(&url)
        .json(decisions)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response)
        .await?
        .json()
        .await
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

/// `POST /api/collection-imports/:id/confirm` — apply the decisions and
/// return the collection's id. A collection changed since the review comes
/// back as a 409.
pub async fn confirm_collection_import(
    archive: &ApiClient,
    staging_id: i64,
    confirm: &ConfirmCollectionImport,
) -> Result<i64, ApiError> {
    let url = archive.url(&format!("/api/collection-imports/{staging_id}/confirm"));
    let response = archive
        .post(&url)
        .json(confirm)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response)
        .await?
        .json::<CreatedCollection>()
        .await
        .map(|created| created.id)
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

/// `DELETE /api/collection-imports/:id` — the owner cancelled.
pub async fn cancel_collection_import(
    archive: &ApiClient,
    staging_id: i64,
) -> Result<(), ApiError> {
    let url = archive.url(&format!("/api/collection-imports/{staging_id}"));
    let response = archive
        .delete(&url)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response).await?;
    Ok(())
}

/// `GET /api/collections/:id/files` — the files a collection keeps, in the
/// order they came.
pub async fn list_collection_files(
    archive: &ApiClient,
    id: i64,
) -> Result<Vec<CollectionFile>, ApiError> {
    get_json(
        archive,
        archive.url(&format!("/api/collections/{id}/files")),
    )
    .await
}

/// Where a kept file is downloaded from — a plain link, as a trip's
/// original is (US-21).
pub fn collection_file_url(archive: &ApiClient, file_id: i64) -> String {
    archive.url(&format!("/api/collection-files/{file_id}/gpx"))
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{import_collection, serve_test_archive, COLLECTION_GPX};
    use trip_archive_types::ImportAction;

    #[tokio::test]
    async fn us88_files_go_up_for_review_and_come_in_once_confirmed() {
        let (archive, _dir) = serve_test_archive().await;
        let (id, _) = import_collection(&archive, COLLECTION_GPX, &[]).await;

        let review = upload_collection_files(
            &archive,
            vec![("again.gpx".to_string(), COLLECTION_GPX.to_vec())],
            Some(id),
        )
        .await
        .unwrap();
        assert!(review
            .tracks
            .iter()
            .all(|track| track.proposed == ImportAction::Skip));
        let reviewed = review_collection_import(
            &archive,
            review.staging_id,
            &ImportDecisions {
                replace_all: true,
                ..ImportDecisions::default()
            },
        )
        .await
        .unwrap();
        assert!(reviewed.replace_all);

        let confirmed = confirm_collection_import(
            &archive,
            review.staging_id,
            &ConfirmCollectionImport {
                held_digest: review.held_digest.clone(),
                ..ConfirmCollectionImport::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(confirmed, id);
        let files = list_collection_files(&archive, id).await.unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[1].file_name, "again.gpx");
        assert!(collection_file_url(&archive, files[1].id).ends_with("/gpx"));
    }

    #[tokio::test]
    async fn us88_a_cancelled_upload_is_gone() {
        let (archive, _dir) = serve_test_archive().await;
        let review = upload_collection_files(
            &archive,
            vec![
                ("a.gpx".to_string(), COLLECTION_GPX.to_vec()),
                ("b.gpx".to_string(), COLLECTION_GPX.to_vec()),
            ],
            None,
        )
        .await
        .unwrap();

        cancel_collection_import(&archive, review.staging_id)
            .await
            .unwrap();

        let again =
            review_collection_import(&archive, review.staging_id, &ImportDecisions::default())
                .await;
        assert!(again.unwrap_err().is_not_found());
    }

    #[tokio::test]
    async fn us88_a_refused_file_comes_back_named() {
        let (archive, _dir) = serve_test_archive().await;

        let err = upload_collection_files(
            &archive,
            vec![("broken.gpx".to_string(), b"<gpx".to_vec())],
            None,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("broken.gpx"), "{err}");
    }
}
