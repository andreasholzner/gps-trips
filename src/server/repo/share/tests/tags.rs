//! US-82 — a share of tags: it reaches the recorded trips under them
//! whenever it is asked, never a planned one.

use super::*;
use crate::server::repo::{add_trip_tag, find_tag_ids, get_or_create_tag, remove_trip_tag};

async fn a_planned_trip(pool: &SqlitePool, name: &str) -> i64 {
    let track = parse_gpx(SAMPLE_GPX).unwrap();
    let stats = compute_stats(&track.points);
    let geojson = build_track_geojson(&track.points);
    insert_trip(
        pool,
        &NewTrip {
            name,
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &stats,
            geojson: &geojson,
            gpx: SAMPLE_GPX,
            trip_kind: TripKind::Planned,
        },
    )
    .await
    .expect("insert_trip")
}

async fn tag(pool: &SqlitePool, trip_id: i64, name: &str) -> i64 {
    let tag_id = get_or_create_tag(pool, name).await.unwrap();
    add_trip_tag(pool, trip_id, tag_id).await.unwrap();
    tag_id
}

async fn a_tag_share(pool: &SqlitePool, token: &str, tags: &[&str]) -> i64 {
    let names: Vec<String> = tags.iter().map(|tag| tag.to_string()).collect();
    let tag_ids = find_tag_ids(pool, &names).await.unwrap().unwrap();
    insert_share(
        pool,
        &NewShare {
            token,
            label: None,
            target: ShareTarget::Tags(&tag_ids),
            created_at: noon(),
            expires_at: None,
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn us82_only_known_tags_are_found_in_the_order_asked() {
    let db = TestDb::new().await;
    let alps = get_or_create_tag(&db.pool, "alps").await.unwrap();
    let norway = get_or_create_tag(&db.pool, "norway").await.unwrap();
    let names = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();

    assert_eq!(
        find_tag_ids(&db.pool, &names(&["norway", "alps"]))
            .await
            .unwrap(),
        Some(vec![norway, alps])
    );
    assert_eq!(
        find_tag_ids(&db.pool, &names(&["alps", "nowhere"]))
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn us82_a_tag_share_reaches_whatever_is_tagged_when_it_is_asked() {
    let db = TestDb::new().await;
    let first = a_trip(&db.pool, "First").await;
    let second = a_trip(&db.pool, "Second").await;
    let elsewhere = a_trip(&db.pool, "Elsewhere").await;
    let alps = tag(&db.pool, first, "alps").await;
    tag(&db.pool, second, "norway").await;
    tag(&db.pool, elsewhere, "home").await;
    let id = a_tag_share(&db.pool, "tok", &["alps", "norway"]).await;

    assert!(share_covers_trip(&db.pool, id, first).await.unwrap());
    assert!(share_covers_trip(&db.pool, id, second).await.unwrap());
    assert!(!share_covers_trip(&db.pool, id, elsewhere).await.unwrap());

    add_trip_tag(&db.pool, elsewhere, alps).await.unwrap();
    remove_trip_tag(&db.pool, first, alps).await.unwrap();
    assert!(share_covers_trip(&db.pool, id, elsewhere).await.unwrap());
    assert!(!share_covers_trip(&db.pool, id, first).await.unwrap());
    let listed: Vec<i64> = list_shared_trips(&db.pool, id)
        .await
        .unwrap()
        .into_iter()
        .map(|trip| trip.id)
        .collect();
    assert_eq!(listed, [second, elsewhere]);
}

#[tokio::test]
async fn us82_a_tag_share_never_reaches_a_planned_trip() {
    let db = TestDb::new().await;
    let planned = a_planned_trip(&db.pool, "Next summer").await;
    tag(&db.pool, planned, "alps").await;
    let id = a_tag_share(&db.pool, "tok", &["alps"]).await;

    assert!(!share_covers_trip(&db.pool, id, planned).await.unwrap());
    assert!(list_shared_trips(&db.pool, id).await.unwrap().is_empty());
}

#[tokio::test]
async fn us82_a_tag_share_covers_its_trips_photos_only() {
    let db = TestDb::new().await;
    let shared = a_trip(&db.pool, "Shared").await;
    let private = a_trip(&db.pool, "Private").await;
    tag(&db.pool, shared, "alps").await;
    a_photo(&db.pool, shared, "trips/1/a.jpg", "trips/1/a_thumb.jpg").await;
    a_photo(&db.pool, private, "trips/2/b.jpg", "trips/2/b_thumb.jpg").await;
    let id = a_tag_share(&db.pool, "tok", &["alps"]).await;

    assert!(share_covers_blob(&db.pool, id, "trips/1/a.jpg")
        .await
        .unwrap());
    assert!(share_covers_blob(&db.pool, id, "trips/1/a_thumb.jpg")
        .await
        .unwrap());
    assert!(!share_covers_blob(&db.pool, id, "trips/2/b.jpg")
        .await
        .unwrap());
}

#[tokio::test]
async fn us82_a_tag_share_without_trips_stays_alive() {
    let db = TestDb::new().await;
    get_or_create_tag(&db.pool, "alps").await.unwrap();
    let id = a_tag_share(&db.pool, "tok", &["alps"]).await;

    assert_eq!(
        resolve_share(&db.pool, "tok", noon())
            .await
            .unwrap()
            .map(|share| share.id),
        Some(id)
    );
    assert_eq!(
        tokens(&list_active_shares(&db.pool, noon()).await.unwrap()),
        ["tok"]
    );
}

#[tokio::test]
async fn us82_a_share_whose_tags_are_all_deleted_ends() {
    let db = TestDb::new().await;
    let alps = get_or_create_tag(&db.pool, "alps").await.unwrap();
    a_tag_share(&db.pool, "tok", &["alps"]).await;

    sqlx::query("DELETE FROM tag WHERE id = ?")
        .bind(alps)
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(resolve_share(&db.pool, "tok", noon()).await.unwrap(), None);
}

#[tokio::test]
async fn us82_the_shared_tags_keep_the_order_they_were_chosen_in() {
    let db = TestDb::new().await;
    get_or_create_tag(&db.pool, "alps").await.unwrap();
    get_or_create_tag(&db.pool, "norway").await.unwrap();
    let id = a_tag_share(&db.pool, "tok", &["norway", "alps"]).await;

    assert_eq!(
        shared_tag_names(&db.pool, id).await.unwrap(),
        ["norway", "alps"]
    );
}

#[tokio::test]
async fn us82_an_active_tag_share_is_listed_with_its_tags() {
    let db = TestDb::new().await;
    let trip = a_trip(&db.pool, "Oslo").await;
    tag(&db.pool, trip, "norway").await;
    get_or_create_tag(&db.pool, "alps").await.unwrap();
    let id = a_tag_share(&db.pool, "tok", &["norway", "alps"]).await;

    let shares = list_active_shares(&db.pool, noon()).await.unwrap();
    assert_eq!(
        shares,
        [ActiveShare {
            id,
            token: "tok".to_string(),
            label: None,
            trip_names: Vec::new(),
            tags: vec!["norway".to_string(), "alps".to_string()],
            created_at: to_rfc3339(noon()),
            expires_at: None,
            opens: 0,
            last_opened_at: None,
            user_agents: Vec::new(),
        }]
    );
}

#[tokio::test]
async fn us82_stopping_a_tag_share_ends_it() {
    let db = TestDb::new().await;
    get_or_create_tag(&db.pool, "alps").await.unwrap();
    let id = a_tag_share(&db.pool, "tok", &["alps"]).await;

    assert!(stop_share(&db.pool, id, noon()).await.unwrap());
    assert_eq!(resolve_share(&db.pool, "tok", noon()).await.unwrap(), None);
    assert!(!stop_share(&db.pool, id, noon()).await.unwrap());
}
