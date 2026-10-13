//! Tagging over the JSON API, against a real archive (ADR-0012): a trip
//! (US-33), a collection or a collection track (US-93), the Tags screen's
//! calls (US-83/85), and many collections at once (US-100). Many trips at
//! once (US-34) is in `api/tests.rs`.

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

// ── US-100: many collections at once ─────────────────────────────────────────

#[tokio::test]
async fn us100_every_selected_collection_carries_every_tag() {
    let (archive, _dir) = serve_test_archive().await;
    let (alps, tracks) = import_collection(&archive, COLLECTION_GPX, &[("name", "Alps")]).await;
    let (tyrol, _) = import_collection(&archive, COLLECTION_GPX, &[("name", "Tyrol")]).await;

    bulk_add_tags(
        &archive,
        BulkTagged::Collections,
        &[alps, tyrol],
        &["alpine".to_string(), "shelf".to_string()],
    )
    .await
    .expect("bulk tag");

    for collection in [alps, tyrol] {
        let names: Vec<String> = list_tags_of(&archive, Tagged::Collection(collection))
            .await
            .expect("tags")
            .into_iter()
            .map(|tag| tag.name)
            .collect();
        assert_eq!(names, ["alpine", "shelf"]);
    }
    assert_eq!(
        list_tags_of(&archive, Tagged::CollectionTrack(tracks[0]))
            .await
            .expect("tags"),
        Vec::new(),
        "never their tracks"
    );
}

#[tokio::test]
async fn us100_a_selection_holding_a_vanished_collection_tags_nothing() {
    let (archive, _dir) = serve_test_archive().await;
    let (alps, _) = import_collection(&archive, COLLECTION_GPX, &[("name", "Alps")]).await;

    let err = bulk_add_tags(
        &archive,
        BulkTagged::Collections,
        &[alps, 9_999],
        &["shelf".to_string()],
    )
    .await
    .expect_err("a vanished collection must fail the request");

    assert!(
        err.to_string()
            .contains("selected collections no longer exist; nothing was tagged"),
        "{err}"
    );
    assert_eq!(
        list_tags_of(&archive, Tagged::Collection(alps))
            .await
            .expect("tags"),
        Vec::new()
    );
}
