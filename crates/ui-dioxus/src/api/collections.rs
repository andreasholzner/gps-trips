//! A guide book's collection and its tracks (US-86): imported, read, edited
//! and deleted over the JSON API, apart from trips (ADR-0028).

use trip_archive_types::{
    CollectionCount, CollectionDetail, CollectionEdit, CollectionPreview, CollectionSummary,
    CollectionTrackDetail, CollectionTrackEdit, CreatedCollection, TripTrack, Waypoint,
};

use super::{get_json, ok_or_error, ApiClient, ApiError};
use crate::track::Track;

/// A guide book's file as a multipart part.
fn gpx_part(file_name: &str, bytes: Vec<u8>) -> Result<reqwest::multipart::Part, ApiError> {
    reqwest::multipart::Part::bytes(bytes)
        .file_name(file_name.to_string())
        .mime_str("application/gpx+xml")
        .map_err(|err| ApiError::new(format!("{file_name} could not be sent: {err}")))
}

/// `POST /api/collections/preview` — what a guide book's file holds and what
/// its details are suggested as, before anything is created. A file the
/// archive cannot use comes back refused, in its words.
pub async fn preview_collection(
    archive: &ApiClient,
    file_name: &str,
    bytes: Vec<u8>,
) -> Result<CollectionPreview, ApiError> {
    let url = archive.url("/api/collections/preview");
    let form = reqwest::multipart::Form::new().part("gpx", gpx_part(file_name, bytes)?);
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

/// `POST /api/collections` — import a guide book's file as a new collection
/// with the details the owner confirmed (an activity's wire value, or empty
/// for none), and return its id.
pub async fn create_collection(
    archive: &ApiClient,
    file_name: &str,
    bytes: Vec<u8>,
    name: &str,
    description: &str,
    activity: &str,
) -> Result<i64, ApiError> {
    let url = archive.url("/api/collections");
    let form = reqwest::multipart::Form::new()
        .part("gpx", gpx_part(file_name, bytes)?)
        .text("name", name.to_string())
        .text("description", description.to_string())
        .text("activity_type", activity.to_string());
    let response = archive
        .post(&url)
        .multipart(form)
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

/// `GET /api/collections` — the collections the trip list's Collections tab
/// lists (US-87), alphabetically. `query` is the name search and region as
/// `Filters::collection_query` writes them, without a leading `?`.
pub async fn list_collections(
    archive: &ApiClient,
    query: String,
) -> Result<Vec<CollectionSummary>, ApiError> {
    let separator = if query.is_empty() { "" } else { "?" };
    get_json(
        archive,
        archive.url(&format!("/api/collections{separator}{query}")),
    )
    .await
}

/// `GET /api/collections/count` — how many collections there are (US-87).
pub async fn count_collections(archive: &ApiClient) -> Result<usize, ApiError> {
    let counted: CollectionCount = get_json(archive, archive.url("/api/collections/count")).await?;
    Ok(usize::try_from(counted.count).unwrap_or_default())
}

/// `GET /api/collection-tracks?ids=…` — the positions of the collection
/// tracks of `ids`, for the trip list's map to draw as lines (US-87). A track
/// there is not is absent. No ids ask for nothing, without a request.
pub async fn list_collection_track_positions(
    archive: &ApiClient,
    ids: &[i64],
) -> Result<Vec<TripTrack>, ApiError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    get_json(
        archive,
        archive.url(&format!("/api/collection-tracks?ids={ids}")),
    )
    .await
}

/// `GET /api/collections/:id` — the details and the tracks in file order. A
/// 404 travels back as such.
pub async fn get_collection(archive: &ApiClient, id: i64) -> Result<CollectionDetail, ApiError> {
    get_json(archive, archive.url(&format!("/api/collections/{id}"))).await
}

/// `GET /api/collections/:id/tracks` — every track's positions, for drawing
/// them on one map.
pub async fn list_collection_tracks(
    archive: &ApiClient,
    id: i64,
) -> Result<Vec<TripTrack>, ApiError> {
    get_json(
        archive,
        archive.url(&format!("/api/collections/{id}/tracks")),
    )
    .await
}

/// `GET /api/collections/:id/waypoints` — every waypoint, in file order.
pub async fn list_collection_waypoints(
    archive: &ApiClient,
    id: i64,
) -> Result<Vec<Waypoint>, ApiError> {
    get_json(
        archive,
        archive.url(&format!("/api/collections/{id}/waypoints")),
    )
    .await
}

/// `PATCH /api/collections/:id` — change the details; a refusal comes back
/// in the archive's words.
pub async fn edit_collection(
    archive: &ApiClient,
    id: i64,
    edit: &CollectionEdit,
) -> Result<(), ApiError> {
    let url = archive.url(&format!("/api/collections/{id}"));
    let response = archive
        .patch(&url)
        .json(edit)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response).await?;
    Ok(())
}

/// `DELETE /api/collections/:id` — the collection with all its tracks and
/// waypoints.
pub async fn delete_collection(archive: &ApiClient, id: i64) -> Result<(), ApiError> {
    let url = archive.url(&format!("/api/collections/{id}"));
    let response = archive
        .delete(&url)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response).await?;
    Ok(())
}

/// `GET /api/collection-tracks/:id` — one track of a collection. A 404
/// travels back as such, as a trip's does.
pub async fn get_collection_track(
    archive: &ApiClient,
    id: i64,
) -> Result<CollectionTrackDetail, ApiError> {
    get_json(
        archive,
        archive.url(&format!("/api/collection-tracks/{id}")),
    )
    .await
}

/// `GET /api/collection-tracks/:id/track.geojson` — the geometry, in the
/// shape a trip's comes in, for the same map and profile.
pub async fn get_collection_track_geometry(
    archive: &ApiClient,
    id: i64,
) -> Result<Track, ApiError> {
    get_json(
        archive,
        archive.url(&format!("/api/collection-tracks/{id}/track.geojson")),
    )
    .await
}

/// `GET /api/collection-tracks/:id/waypoints` — the waypoints that belong to
/// the track.
pub async fn list_collection_track_waypoints(
    archive: &ApiClient,
    id: i64,
) -> Result<Vec<Waypoint>, ApiError> {
    get_json(
        archive,
        archive.url(&format!("/api/collection-tracks/{id}/waypoints")),
    )
    .await
}

/// `PATCH /api/collection-tracks/:id` — rename the track or give it an
/// activity of its own; a refusal comes back in the archive's words.
pub async fn edit_collection_track(
    archive: &ApiClient,
    id: i64,
    edit: &CollectionTrackEdit,
) -> Result<(), ApiError> {
    let url = archive.url(&format!("/api/collection-tracks/{id}"));
    let response = archive
        .patch(&url)
        .json(edit)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response).await?;
    Ok(())
}

/// `DELETE /api/collection-tracks/:id` — one track; its waypoints stay with
/// the collection.
pub async fn delete_collection_track(archive: &ApiClient, id: i64) -> Result<(), ApiError> {
    let url = archive.url(&format!("/api/collection-tracks/{id}"));
    let response = archive
        .delete(&url)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response).await?;
    Ok(())
}

/// Where the track's own GPX is downloaded from — a plain link, as a trip's
/// original is (US-21).
pub fn collection_track_gpx_url(archive: &ApiClient, id: i64) -> String {
    archive.url(&format!("/api/collection-tracks/{id}/gpx"))
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{import_collection, serve_test_archive, GUIDE_BOOK_GPX};

    #[tokio::test]
    async fn us87_the_collections_are_listed_narrowed_by_the_query() {
        let (archive, _dir) = serve_test_archive().await;
        let (lakes, _) = import_collection(&archive, GUIDE_BOOK_GPX, &[("name", "Lakes")]).await;
        import_collection(&archive, GUIDE_BOOK_GPX, &[("name", "Passes")]).await;

        let all = list_collections(&archive, String::new()).await.unwrap();
        let narrowed = list_collections(&archive, "q=lak".to_string())
            .await
            .unwrap();

        assert_eq!(all.len(), 2);
        assert_eq!(narrowed.len(), 1);
        assert_eq!(narrowed[0].id, lakes);
        assert_eq!(narrowed[0].tracks.len(), 3);
    }

    #[tokio::test]
    async fn us87_the_collections_are_counted() {
        let (archive, _dir) = serve_test_archive().await;
        import_collection(&archive, GUIDE_BOOK_GPX, &[]).await;
        import_collection(&archive, GUIDE_BOOK_GPX, &[]).await;

        assert_eq!(count_collections(&archive).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn us87_the_tracks_in_view_are_read_in_one_request() {
        let (archive, _dir) = serve_test_archive().await;
        let (_, tracks) = import_collection(&archive, GUIDE_BOOK_GPX, &[]).await;

        let answer = list_collection_track_positions(&archive, &tracks[1..])
            .await
            .unwrap();

        let ids: Vec<i64> = answer.iter().map(|track| track.id).collect();
        assert_eq!(ids, tracks[1..]);
        assert!(list_collection_track_positions(&archive, &[])
            .await
            .unwrap()
            .is_empty());
    }
}
