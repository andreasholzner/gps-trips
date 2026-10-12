//! US-93 — tags on collections and their tracks, through links of their own
//! (ADR-0028).

use super::*;
use crate::models::TagOverview;
use crate::server::repo::{
    delete_collection, delete_collection_track, get_collection, insert_collection, NewCollection,
    NewCollectionFile, NewCollectionTrack,
};

fn noon() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap()
}

/// A collection named `name` with two tracks; its id and its tracks' ids.
async fn insert_book(pool: &SqlitePool, name: &str) -> (i64, Vec<i64>) {
    let track = parse_gpx(SAMPLE_GPX).unwrap();
    let stats = compute_stats(&track.points);
    let geojson = build_track_geojson(&track.points);
    let tracks: Vec<NewCollectionTrack> = ["Day 1", "Day 2"]
        .iter()
        .map(|name| NewCollectionTrack {
            name,
            stats: &stats,
            geojson: &geojson,
            gpx: b"<gpx/>",
        })
        .collect();
    let id = insert_collection(
        pool,
        &NewCollection {
            name,
            description: "",
            activity_type: None,
            files: &[NewCollectionFile {
                file_name: "sample.gpx",
                gpx: SAMPLE_GPX,
            }],
            tracks: &tracks,
            waypoints: &[],
        },
    )
    .await
    .expect("insert_collection");
    let detail = get_collection(pool, id).await.unwrap().unwrap();
    (id, detail.tracks.iter().map(|track| track.id).collect())
}

async fn tag(pool: &SqlitePool, tagged: Tagged, name: &str) -> i64 {
    let tag_id = get_or_create_tag(pool, name).await.unwrap();
    add_tag(pool, tagged, tag_id).await.unwrap();
    tag_id
}

async fn names(pool: &SqlitePool, tagged: Tagged) -> Vec<String> {
    list_tags_of(pool, tagged)
        .await
        .unwrap()
        .into_iter()
        .map(|tag| tag.name)
        .collect()
}

#[tokio::test]
async fn us93_a_collections_tags_and_its_tracks_tags_are_their_own() {
    let db = TestDb::new().await;
    let (book, tracks) = insert_book(&db.pool, "Alpencross").await;
    let trip = insert_sample_trip(&db.pool).await;
    tag(&db.pool, Tagged::Collection(book), "alps").await;
    tag(&db.pool, Tagged::CollectionTrack(tracks[0]), "summit").await;
    tag(&db.pool, Tagged::CollectionTrack(tracks[0]), "hut").await;

    assert_eq!(names(&db.pool, Tagged::Collection(book)).await, ["alps"]);
    assert_eq!(
        names(&db.pool, Tagged::CollectionTrack(tracks[0])).await,
        ["hut", "summit"]
    );
    assert!(names(&db.pool, Tagged::CollectionTrack(tracks[1]))
        .await
        .is_empty());
    // The ids are the same numbers in different tables: a trip with a
    // collection's id carries none of its tags.
    assert_eq!(trip, book);
    assert!(names(&db.pool, Tagged::Trip(trip)).await.is_empty());
}

#[tokio::test]
async fn us93_tagging_twice_is_a_no_op_and_untagging_keeps_the_tag() {
    let db = TestDb::new().await;
    let (book, tracks) = insert_book(&db.pool, "Alpencross").await;
    let alps = tag(&db.pool, Tagged::Collection(book), "alps").await;
    add_tag(&db.pool, Tagged::Collection(book), alps)
        .await
        .unwrap();
    tag(&db.pool, Tagged::CollectionTrack(tracks[0]), "alps").await;

    assert!(
        remove_tag(&db.pool, Tagged::CollectionTrack(tracks[0]), alps)
            .await
            .unwrap()
    );
    assert!(
        !remove_tag(&db.pool, Tagged::CollectionTrack(tracks[0]), alps)
            .await
            .unwrap()
    );

    assert_eq!(names(&db.pool, Tagged::Collection(book)).await, ["alps"]);
    assert!(names(&db.pool, Tagged::CollectionTrack(tracks[0]))
        .await
        .is_empty());
    assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn us93_the_overview_counts_collections_and_tracks_whose_collection_lacks_the_tag() {
    let db = TestDb::new().await;
    let trip = insert_sample_trip(&db.pool).await;
    let (tagged_book, tagged_book_tracks) = insert_book(&db.pool, "Alpencross").await;
    let (_, other_tracks) = insert_book(&db.pool, "Dolomites").await;
    let alps = tag(&db.pool, Tagged::Trip(trip), "alps").await;
    tag(&db.pool, Tagged::Collection(tagged_book), "alps").await;
    // Its collection carries the tag already: not counted again.
    tag(
        &db.pool,
        Tagged::CollectionTrack(tagged_book_tracks[0]),
        "alps",
    )
    .await;
    for track in &other_tracks {
        tag(&db.pool, Tagged::CollectionTrack(*track), "alps").await;
    }
    let tracks_only = tag(&db.pool, Tagged::CollectionTrack(other_tracks[0]), "pass").await;

    assert_eq!(
        list_tag_overview(&db.pool, noon()).await.unwrap(),
        vec![
            TagOverview {
                id: alps,
                name: "alps".to_string(),
                trip_count: 1,
                recorded_trip_count: 1,
                collection_count: 1,
                collection_track_count: 2,
                shares: Vec::new(),
            },
            TagOverview {
                id: tracks_only,
                name: "pass".to_string(),
                trip_count: 0,
                recorded_trip_count: 0,
                collection_count: 0,
                collection_track_count: 1,
                shares: Vec::new(),
            },
        ]
    );
}

#[tokio::test]
async fn us93_deleting_a_collection_or_a_track_takes_its_links_and_keeps_the_tag() {
    let db = TestDb::new().await;
    let (book, tracks) = insert_book(&db.pool, "Alpencross").await;
    tag(&db.pool, Tagged::Collection(book), "alps").await;
    tag(&db.pool, Tagged::CollectionTrack(tracks[0]), "alps").await;
    tag(&db.pool, Tagged::CollectionTrack(tracks[1]), "alps").await;

    delete_collection_track(&db.pool, tracks[0]).await.unwrap();
    let overview = list_tag_overview(&db.pool, noon()).await.unwrap();
    assert_eq!(
        (
            overview[0].collection_count,
            overview[0].collection_track_count
        ),
        (1, 0)
    );

    delete_collection(&db.pool, book).await.unwrap();
    let links: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM collection_tag) \
              + (SELECT COUNT(*) FROM collection_track_tag)",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(links, 0);
    assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn us93_renaming_or_deleting_a_tag_reaches_collections_and_tracks() {
    let db = TestDb::new().await;
    let (book, tracks) = insert_book(&db.pool, "Alpencross").await;
    let alps = tag(&db.pool, Tagged::Collection(book), "alps").await;
    tag(&db.pool, Tagged::CollectionTrack(tracks[0]), "alps").await;

    rename_tag(&db.pool, alps, "alpen").await.unwrap();
    assert_eq!(names(&db.pool, Tagged::Collection(book)).await, ["alpen"]);
    assert_eq!(
        names(&db.pool, Tagged::CollectionTrack(tracks[0])).await,
        ["alpen"]
    );

    assert!(delete_tag(&db.pool, alps).await.unwrap());
    assert!(names(&db.pool, Tagged::Collection(book)).await.is_empty());
    assert!(names(&db.pool, Tagged::CollectionTrack(tracks[0]))
        .await
        .is_empty());
    assert!(get_collection(&db.pool, book).await.unwrap().is_some());
}

#[tokio::test]
async fn us93_tagging_by_name_creates_the_tag_and_links_it() {
    let db = TestDb::new().await;
    let (book, _) = insert_book(&db.pool, "Alpencross").await;

    let tagged = add_tag_named(&db.pool, Tagged::Collection(book), "alps")
        .await
        .unwrap()
        .expect("the collection exists");

    assert_eq!(tagged.name, "alps");
    assert_eq!(
        list_tags_of(&db.pool, Tagged::Collection(book))
            .await
            .unwrap(),
        vec![tagged]
    );
}

#[tokio::test]
async fn us93_tagging_something_gone_by_name_creates_nothing() {
    // Checked in the same transaction as the link is made, so something
    // deleted after a request's own check is not found rather than a
    // foreign-key failure — and leaves no tag behind.
    let db = TestDb::new().await;

    for gone in [
        Tagged::Trip(999),
        Tagged::Collection(999),
        Tagged::CollectionTrack(999),
    ] {
        assert_eq!(
            add_tag_named(&db.pool, gone, "alps").await.unwrap(),
            None,
            "{gone:?}"
        );
    }
    assert!(list_all_tags(&db.pool).await.unwrap().is_empty());
}
