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
mod tests {
    use super::*;
    use crate::api::list_tags;
    use crate::test_support::{
        import_collection, import_sample, serve_test_archive, tag_trip, COLLECTION_GPX,
    };

    // ── US-33: tagging a trip from its detail screen ─────────────────────

    #[tokio::test]
    async fn a_new_tag_is_created_by_using_it_and_normalized_as_it_is_stored() {
        // "Tag names are normalized (trimmed, lowercased) so casing doesn't
        // create duplicates."
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;

        let created = add_tag(&archive, Tagged::Trip(id), "  Alpine  ")
            .await
            .expect("tag");
        assert_eq!(created.name, "alpine");

        // The same name in another casing joins the tag that exists rather
        // than making a second one.
        add_tag(&archive, Tagged::Trip(id), "ALPINE")
            .await
            .expect("tag");
        let names: Vec<String> = list_tags(&archive)
            .await
            .expect("tags")
            .into_iter()
            .map(|tag| tag.name)
            .collect();
        assert_eq!(names, vec!["alpine".to_string()]);
    }

    #[tokio::test]
    async fn untagging_a_trip_keeps_the_tag_for_next_time() {
        // "Untagging a trip keeps the now-unused tag row around, suggestible
        // again later."
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;
        let tag = add_tag(&archive, Tagged::Trip(id), "alpine")
            .await
            .expect("tag");

        remove_tag(&archive, Tagged::Trip(id), tag.id)
            .await
            .expect("untag");

        assert!(
            list_tags_of(&archive, Tagged::Trip(id))
                .await
                .expect("trip tags")
                .is_empty(),
            "the trip is untagged"
        );
        assert_eq!(
            list_tags(&archive).await.expect("tags").len(),
            1,
            "the tag itself outlives the trip that used it"
        );
    }

    #[tokio::test]
    async fn a_name_the_archive_refuses_is_reported_in_its_own_words() {
        let (archive, _dir) = serve_test_archive().await;
        let id = import_sample(&archive, &[]).await;

        let err = add_tag(&archive, Tagged::Trip(id), "day trip")
            .await
            .expect_err("a name with a space is not a tag");

        assert!(err.to_string().contains("cannot contain spaces"), "{err}");
        assert!(list_tags(&archive).await.expect("tags").is_empty());
    }

    // ── US-93: tagging a collection and a collection track ──────────

    #[tokio::test]
    async fn us93_a_collection_and_its_track_are_tagged_and_counted_apart() {
        let (archive, _dir) = serve_test_archive().await;
        let (id, tracks) = import_collection(&archive, COLLECTION_GPX, &[]).await;

        let alps = add_tag(&archive, Tagged::Collection(id), " Alps")
            .await
            .expect("tag the collection");
        assert_eq!(alps.name, "alps");
        add_tag(&archive, Tagged::CollectionTrack(tracks[0]), "pass")
            .await
            .expect("tag the track");

        let names = |tags: Vec<Tag>| tags.into_iter().map(|tag| tag.name).collect::<Vec<_>>();
        let on_collection = list_tags_of(&archive, Tagged::Collection(id)).await;
        assert_eq!(names(on_collection.expect("collection tags")), ["alps"]);
        let on_track = list_tags_of(&archive, Tagged::CollectionTrack(tracks[0])).await;
        assert_eq!(names(on_track.expect("track tags")), ["pass"]);
        let listed = tag_overview(&archive).await.expect("overview");
        let counts: Vec<(i64, i64)> = listed
            .iter()
            .map(|tag| (tag.collection_count, tag.collection_track_count))
            .collect();
        assert_eq!(counts, [(1, 0), (0, 1)]);

        remove_tag(&archive, Tagged::Collection(id), alps.id)
            .await
            .expect("untag");
        let on_collection = list_tags_of(&archive, Tagged::Collection(id)).await;
        assert!(on_collection.expect("collection tags").is_empty());
    }

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

    #[tokio::test]
    async fn us85_a_renamed_tag_keeps_its_trips_under_the_new_name() {
        let (archive, _dir) = serve_test_archive().await;
        let trip = import_sample(&archive, &[]).await;
        tag_trip(&archive, trip, "alps").await;
        let alps = tag_overview(&archive).await.expect("overview")[0].id;

        let renamed = rename_tag(&archive, alps, " Alpen").await.expect("rename");

        assert_eq!((renamed.id, renamed.name.as_str()), (alps, "alpen"));
        let listed = tag_overview(&archive).await.expect("overview");
        assert_eq!(
            (listed[0].name.as_str(), listed[0].trip_count),
            ("alpen", 1)
        );
    }

    #[tokio::test]
    async fn us85_a_refused_new_name_comes_back_in_the_archives_words() {
        let (archive, _dir) = serve_test_archive().await;
        let alps = create_tag(&archive, "alps").await.expect("create");
        create_tag(&archive, "norway").await.expect("create");

        let existing = rename_tag(&archive, alps.id, "Norway").await.unwrap_err();
        let invalid = rename_tag(&archive, alps.id, "day trip").await.unwrap_err();

        assert_eq!(existing.to_string(), "tag \"norway\" already exists");
        assert_eq!(invalid.to_string(), "tag name cannot contain spaces");
    }
}
