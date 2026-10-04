//! Many trips' tracks in one request (US-73), for drawing them as lines —
//! the owner's list map, and a share's overview map through a client made
//! with [`ApiClient::for_share`].
//! Only the positions travel; the detail screen still reads a trip's whole
//! `track.geojson` with [`get_track`](super::get_track), and its climbs
//! (US-81) with [`list_climbs`].

use trip_archive_types::{Climb, TripTrack};

use super::{get_json, ApiClient, ApiError};

/// `GET /api/trips/tracks?ids=…` — the tracks of `ids`, as `[lon, lat]`
/// positions. A trip whose track cannot be read is simply absent. No ids ask
/// for nothing, without a request.
pub async fn list_tracks(archive: &ApiClient, ids: &[i64]) -> Result<Vec<TripTrack>, ApiError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    get_json(
        archive,
        archive.url(&format!("/api/trips/tracks?ids={ids}")),
    )
    .await
}

/// `GET /api/trips/:id/climbs` — the trip's climbs in track order (US-81),
/// for the owner or, through a share's client, its recipient.
pub async fn list_climbs(archive: &ApiClient, id: i64) -> Result<Vec<Climb>, ApiError> {
    get_json(archive, archive.url(&format!("/api/trips/{id}/climbs"))).await
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{create_share, get_track};
    use crate::test_support::{anonymous, import_sample, serve_test_archive};
    use trip_archive_types::{CreateShare, ShareExpiry};

    #[tokio::test]
    async fn the_tracks_in_view_come_in_one_request_as_lon_lat() {
        let (archive, _dir) = serve_test_archive().await;
        let first = import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
        let second = import_sample(&archive, &[("name", "Inn Valley Ride")]).await;

        let tracks = list_tracks(&archive, &[first, second])
            .await
            .expect("tracks");

        let ids: Vec<i64> = tracks.iter().map(|track| track.id).collect();
        assert_eq!(ids, [first, second]);
        let stored = get_track(&archive, first).await.expect("track");
        let [lon, lat, ..] = stored.geometry.coordinates[0][..] else {
            panic!("a stored position has a lon and a lat");
        };
        assert_eq!(tracks[0].coordinates[0], [lon, lat]);
    }

    #[tokio::test]
    async fn through_a_share_only_its_own_trips_tracks_come() {
        let (archive, _dir) = serve_test_archive().await;
        let shared = import_sample(&archive, &[("name", "Oslo Hills Walk")]).await;
        let other = import_sample(&archive, &[("name", "Inn Valley Ride")]).await;
        let created = create_share(
            &archive,
            &CreateShare {
                trip_ids: vec![shared],
                tags: Vec::new(),
                label: None,
                expiry: ShareExpiry::Never,
            },
        )
        .await
        .expect("share");
        let recipient = anonymous(&archive).for_share(created.token);

        let tracks = list_tracks(&recipient, &[shared, other])
            .await
            .expect("tracks");

        let ids: Vec<i64> = tracks.iter().map(|track| track.id).collect();
        assert_eq!(ids, [shared]);
    }

    #[tokio::test]
    async fn no_trips_in_view_ask_for_no_tracks() {
        let (archive, _dir) = serve_test_archive().await;

        assert!(list_tracks(&archive, &[]).await.expect("tracks").is_empty());
    }

    #[tokio::test]
    async fn us81_a_trips_climbs_are_read_by_its_owner_and_through_its_share() {
        let (archive, _dir) = serve_test_archive().await;
        let id = crate::test_support::import_gpx(
            &archive,
            crate::test_support::HILL_GPX,
            &[("activity_type", "hiking")],
        )
        .await;
        let share = create_share(
            &archive,
            &CreateShare {
                trip_ids: vec![id],
                tags: Vec::new(),
                label: None,
                expiry: ShareExpiry::Never,
            },
        )
        .await
        .expect("share");
        let recipient = anonymous(&archive).for_share(share.token);

        let owners = list_climbs(&archive, id).await.expect("the owner's");
        let shared = list_climbs(&recipient, id).await.expect("the recipient's");

        assert_eq!(owners.len(), 1, "{owners:?}");
        assert_eq!(shared, owners);
    }
}
