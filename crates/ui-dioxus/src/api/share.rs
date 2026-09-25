//! Shares (US-53): the owner making one, and the recipient reading it. The
//! recipient's calls go through a client made with
//! [`ApiClient::for_share`], which is what puts them under the share's own
//! routes; the track and the photos are read with the ordinary calls on that
//! same client.

use trip_archive_types::{CreateShare, CreatedShare, ShareOverview, SharedTrip};

use super::{get_json, ok_or_error, ApiClient, ApiError};

/// `POST /api/shares` — share a few trips (the owner's call).
pub async fn create_share(
    archive: &ApiClient,
    request: &CreateShare,
) -> Result<CreatedShare, ApiError> {
    let url = archive.url("/api/shares");
    let response = archive
        .post(&url)
        .json(request)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response)
        .await?
        .json()
        .await
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

/// The link a share's token opens, on the archive `archive` reaches.
pub fn share_link(archive: &ApiClient, token: &str) -> String {
    format!("{}/app/s/{token}", archive.base_url())
}

/// `GET /s/:token/api/share` — the share's title and its trips.
pub async fn share_overview(archive: &ApiClient) -> Result<ShareOverview, ApiError> {
    get_json(archive, archive.url("/api/share")).await
}

/// `GET /s/:token/api/trips/:id` — one shared trip.
pub async fn get_shared_trip(archive: &ApiClient, id: i64) -> Result<SharedTrip, ApiError> {
    get_json(archive, archive.url(&format!("/api/trips/{id}"))).await
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{get_track, list_photos, original_gpx_url};
    use crate::test_support::{anonymous, import_sample, serve_test_archive};
    use trip_archive_types::ShareExpiry;

    #[tokio::test]
    async fn us53_the_owner_shares_a_trip_and_the_link_reads_it_without_a_session() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;

        let created = create_share(
            &archive,
            &CreateShare {
                trip_ids: vec![id],
                label: Some("For Kari".to_string()),
                expiry: ShareExpiry::OneMonth,
            },
        )
        .await
        .expect("share");
        assert!(created.expires_at.is_some());
        assert_eq!(
            share_link(&archive, &created.token),
            format!("{}/app/s/{}", archive.base_url(), created.token)
        );

        let recipient = anonymous(&archive).for_share(created.token);
        let overview = share_overview(&recipient).await.expect("overview");
        assert_eq!(overview.label.as_deref(), Some("For Kari"));
        assert_eq!(overview.trips[0].name, "Oslo Hills Walk");
        assert_eq!(
            get_shared_trip(&recipient, id).await.expect("trip").name,
            "Oslo Hills Walk"
        );
        get_track(&recipient, id).await.expect("track");
        list_photos(&recipient, id).await.expect("photos");
        let gpx = reqwest::get(original_gpx_url(&recipient, id))
            .await
            .expect("gpx");
        assert!(gpx.status().is_success(), "{}", gpx.status());
    }

    #[tokio::test]
    async fn us53_a_dead_link_reads_as_not_found() {
        let (archive, _dir) = serve_test_archive().await;
        let err = share_overview(&anonymous(&archive).for_share("nope"))
            .await
            .unwrap_err();
        assert!(err.is_not_found(), "{err}");
    }
}
