//! Tagging a trip (US-33), a collection or a collection track (US-93), and
//! the Tags screen's calls (US-83): every tag with what carries it and its
//! shares, creating a tag, renaming one (US-85) and deleting one.

use serde::Serialize;
use trip_archive_types::{Tag, TagOverview};

use super::{get_json, ok_or_error, ApiClient, ApiError};

/// What a tag is put on: each has its own `…/:id/tags` routes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tagged {
    Trip(i64),
    Collection(i64),
    CollectionTrack(i64),
}

impl Tagged {
    fn tags_url(self, archive: &ApiClient) -> String {
        archive.url(&match self {
            Tagged::Trip(id) => format!("/api/trips/{id}/tags"),
            Tagged::Collection(id) => format!("/api/collections/{id}/tags"),
            Tagged::CollectionTrack(id) => format!("/api/collection-tracks/{id}/tags"),
        })
    }
}

/// `GET …/:id/tags` — the tags it carries.
pub async fn list_tags_of(archive: &ApiClient, tagged: Tagged) -> Result<Vec<Tag>, ApiError> {
    get_json(archive, tagged.tags_url(archive)).await
}

/// `POST …/:id/tags` — tag it, creating the tag if this is the first use of
/// the name. The archive's own words come back for a name it refuses (one
/// with a space or a comma), so the owner reads the rule rather than a
/// status code.
pub async fn add_tag(archive: &ApiClient, tagged: Tagged, name: &str) -> Result<Tag, ApiError> {
    let url = tagged.tags_url(archive);
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

/// What the list screens select and tag together: trips (US-34), or
/// collections (US-100) — never collection tracks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BulkTagged {
    Trips,
    Collections,
}

impl BulkTagged {
    /// What is selected, as the screens and messages name it.
    pub fn plural(self) -> &'static str {
        match self {
            BulkTagged::Trips => "trips",
            BulkTagged::Collections => "collections",
        }
    }
}

/// The `POST /api/trips/tags` body (US-34): every name applied to every
/// trip, in one request.
#[derive(Serialize)]
struct BulkAddTripTags<'a> {
    trip_ids: &'a [i64],
    names: &'a [String],
}

/// The `POST /api/collections/tags` body (US-100), as the trips' is.
#[derive(Serialize)]
struct BulkAddCollectionTags<'a> {
    collection_ids: &'a [i64],
    names: &'a [String],
}

/// `POST /api/trips/tags` or `POST /api/collections/tags` — tag everything
/// selected, `ids` of `of`, with every staged name (US-34, US-100).
/// All-or-nothing: if any id no longer exists the server tags nothing, which
/// this reports as such rather than as a bare 404. Other failures carry the
/// server's own message (an invalid name, US-33's rules) so the owner reads
/// what is wrong instead of a status code.
pub async fn bulk_add_tags(
    archive: &ApiClient,
    of: BulkTagged,
    ids: &[i64],
    names: &[String],
) -> Result<Vec<Tag>, ApiError> {
    let url = archive.url(match of {
        BulkTagged::Trips => "/api/trips/tags",
        BulkTagged::Collections => "/api/collections/tags",
    });
    let request = archive.post(&url);
    let request = match of {
        BulkTagged::Trips => request.json(&BulkAddTripTags {
            trip_ids: ids,
            names,
        }),
        BulkTagged::Collections => request.json(&BulkAddCollectionTags {
            collection_ids: ids,
            names,
        }),
    };
    let response = request
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;

    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(ApiError::from_status(
            status,
            format!(
                "one or more selected {} no longer exist; nothing was tagged",
                of.plural()
            ),
        ));
    }
    ok_or_error(archive, &url, response)
        .await?
        .json()
        .await
        .map_err(|err| ApiError::new(format!("{url} returned unreadable JSON: {err}")))
}

/// `DELETE …/:id/tags/:tag_id` — take a tag off. The tag itself stays in the
/// archive, unused but suggestible again later.
pub async fn remove_tag(archive: &ApiClient, tagged: Tagged, tag_id: i64) -> Result<(), ApiError> {
    let url = format!("{}/{tag_id}", tagged.tags_url(archive));
    let response = archive
        .delete(&url)
        .send()
        .await
        .map_err(|err| ApiError::new(format!("{url} unreachable: {err}")))?;
    ok_or_error(archive, &url, response).await?;
    Ok(())
}

/// `GET /api/tags/overview` — every tag, alphabetical, with how many trips
/// carry it and the active summary shares naming it.
pub async fn tag_overview(archive: &ApiClient) -> Result<Vec<TagOverview>, ApiError> {
    get_json(archive, archive.url("/api/tags/overview")).await
}

/// The body of every call naming a tag — tagging something, creating a tag,
/// renaming one: the name as typed. The archive normalizes it — trimmed and
/// lowercased (US-33) — so casing never makes a second tag.
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

/// `PATCH /api/tags/:id` — rename a tag; its trips and shares carry the new
/// name. A refused name comes back in the archive's own words, as
/// `create_tag`'s does.
pub async fn rename_tag(archive: &ApiClient, id: i64, name: &str) -> Result<Tag, ApiError> {
    let url = archive.url(&format!("/api/tags/{id}"));
    let response = archive
        .patch(&url)
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
mod tests;
