//! Links from what the owner did to what they planned (US-99): a selection
//! linked in one go, what a trip, a collection track or a waypoint is linked
//! to, and a link removed from either end — always named from its trip's.

use trip_archive_types::{CreateLinks, CreatedLinks, LinkOther, LinkPair, Links, SharedLinks};

use super::{get_json, ok_or_error, ApiClient, ApiError};

/// What a page lists the links of, or what a trip's link ends at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LinkEnd {
    Trip(i64),
    CollectionTrack(i64),
    Waypoint(i64),
}

impl LinkEnd {
    /// Its own route's path, `/api/…/:id`.
    fn path(self) -> String {
        match self {
            LinkEnd::Trip(id) => format!("/api/trips/{id}"),
            LinkEnd::CollectionTrack(id) => format!("/api/collection-tracks/{id}"),
            LinkEnd::Waypoint(id) => format!("/api/waypoints/{id}"),
        }
    }

    /// The link or proposal between `trip_id` and this end, as confirming or
    /// dismissing a proposal names it (US-103).
    pub fn pair_with(self, trip_id: i64) -> LinkPair {
        let other = match self {
            LinkEnd::Trip(id) => LinkOther::Trip(id),
            LinkEnd::CollectionTrack(id) => LinkOther::CollectionTrack(id),
            LinkEnd::Waypoint(id) => LinkOther::Waypoint(id),
        };
        LinkPair { trip_id, other }
    }

    /// Its segment under a trip's `…/links/`.
    fn segment(self) -> String {
        match self {
            LinkEnd::Trip(id) => format!("trips/{id}"),
            LinkEnd::CollectionTrack(id) => format!("collection-tracks/{id}"),
            LinkEnd::Waypoint(id) => format!("waypoints/{id}"),
        }
    }
}

/// `POST /api/links` — link every pair the selection holds that can be
/// linked. A refusal comes back in the archive's words.
pub async fn create_links(
    archive: &ApiClient,
    request: &CreateLinks,
) -> Result<CreatedLinks, ApiError> {
    let url = archive.url("/api/links");
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

/// `GET …/:id/links` — what it is linked to.
pub async fn list_links(archive: &ApiClient, end: LinkEnd) -> Result<Links, ApiError> {
    get_json(archive, archive.url(&format!("{}/links", end.path()))).await
}

/// `DELETE /api/trips/:id/links/…` — remove the trip's link to `other`.
pub async fn unlink(archive: &ApiClient, trip_id: i64, other: LinkEnd) -> Result<(), ApiError> {
    let url = archive.url(&format!("/api/trips/{trip_id}/links/{}", other.segment()));
    let response = archive
        .delete(&url)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response).await?;
    Ok(())
}

/// `GET /s/:token/api/trips/:id/links` — the shared trips a shared trip is
/// linked to, on a client made for the share.
pub async fn list_shared_trip_links(archive: &ApiClient, id: i64) -> Result<SharedLinks, ApiError> {
    get_json(archive, archive.url(&format!("/api/trips/{id}/links"))).await
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{create_share, list_collection_waypoints};
    use crate::test_support::{
        import_collection, import_sample, serve_test_archive, COLLECTION_GPX,
    };
    use trip_archive_types::CreateShare;

    #[tokio::test]
    async fn us99_a_selection_is_linked_listed_and_unlinked() {
        let (archive, _dir) = serve_test_archive().await;
        let monday = import_sample(&archive, &[]).await;
        let plan = import_sample(&archive, &[("kind", "planned")]).await;
        let (_, tracks) = import_collection(&archive, COLLECTION_GPX, &[]).await;

        let created = create_links(
            &archive,
            &CreateLinks {
                trip_ids: vec![monday, plan],
                collection_track_ids: vec![tracks[0]],
            },
        )
        .await
        .unwrap();

        assert_eq!(created.pairs, 3);
        let on_track = list_links(&archive, LinkEnd::CollectionTrack(tracks[0]))
            .await
            .unwrap();
        assert_eq!(on_track.recorded_trips[0].id, monday);
        assert_eq!(on_track.planned_trips[0].id, plan);
        unlink(&archive, plan, LinkEnd::Trip(monday)).await.unwrap();
        unlink(&archive, monday, LinkEnd::CollectionTrack(tracks[0]))
            .await
            .unwrap();
        let on_monday = list_links(&archive, LinkEnd::Trip(monday)).await.unwrap();
        assert_eq!(on_monday, Links::default());
    }

    #[tokio::test]
    async fn us99_a_selection_with_nothing_to_link_is_refused_in_the_archives_words() {
        let (archive, _dir) = serve_test_archive().await;
        let monday = import_sample(&archive, &[]).await;

        let err = create_links(
            &archive,
            &CreateLinks {
                trip_ids: vec![monday],
                collection_track_ids: vec![],
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("can be linked"), "{err}");
    }

    #[tokio::test]
    async fn us99_a_waypoints_links_are_read_and_a_recipient_reads_a_shared_trips() {
        let (archive, _dir) = serve_test_archive().await;
        let monday = import_sample(&archive, &[]).await;
        let plan = import_sample(&archive, &[("kind", "planned")]).await;
        let (collection, _) = import_collection(&archive, COLLECTION_GPX, &[]).await;
        let waypoint = list_collection_waypoints(&archive, collection)
            .await
            .unwrap()[0]
            .id;
        create_links(
            &archive,
            &CreateLinks {
                trip_ids: vec![monday, plan],
                collection_track_ids: vec![],
            },
        )
        .await
        .unwrap();
        let share = create_share(
            &archive,
            &CreateShare {
                trip_ids: vec![monday, plan],
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let on_waypoint = list_links(&archive, LinkEnd::Waypoint(waypoint))
            .await
            .unwrap();
        let shared = list_shared_trip_links(&archive.clone().for_share(share.token), monday)
            .await
            .unwrap();

        assert_eq!(on_waypoint, Links::default());
        assert_eq!(shared.planned_trips[0].id, plan);
    }
}
