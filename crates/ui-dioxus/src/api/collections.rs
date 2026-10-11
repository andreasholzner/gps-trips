//! A guide book's collection and its tracks (US-86): read, edited and
//! deleted over the JSON API, apart from trips (ADR-0028).

use trip_archive_types::{CollectionTrackDetail, CollectionTrackEdit, Waypoint};

use super::{get_json, ok_or_error, ApiClient, ApiError};
use crate::track::Track;

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
