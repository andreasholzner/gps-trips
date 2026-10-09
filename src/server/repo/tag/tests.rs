//! US-33/34/83/85 — the tag repository, against a real SQLite file (ADR-0012).

use super::*;
use crate::models::{ActivityType, TripKind};
use crate::server::db::testing::TestDb;
use crate::server::geojson::build_track_geojson;
use crate::server::gpx::{compute_stats, parse_gpx};
use crate::server::repo::{insert_trip, NewTrip};

const SAMPLE_GPX: &[u8] = include_bytes!("../../../../tests/fixtures/sample.gpx");

async fn insert_sample_trip(pool: &SqlitePool) -> i64 {
    let track = parse_gpx(SAMPLE_GPX).unwrap();
    let stats = compute_stats(&track.points);
    let geojson = build_track_geojson(&track.points);
    insert_trip(
        pool,
        &NewTrip {
            name: "Oslo Hills Walk",
            activity_type: ActivityType::Hiking,
            tz_name: "Europe/Oslo",
            stats: &stats,
            geojson: &geojson,
            gpx: SAMPLE_GPX,
            trip_kind: TripKind::Recorded,
        },
    )
    .await
    .expect("insert_trip")
}

#[tokio::test]
async fn get_or_create_tag_creates_a_new_tag() {
    let db = TestDb::new().await;
    let id = get_or_create_tag(&db.pool, "hiking").await.unwrap();
    assert!(id > 0);

    let all = list_all_tags(&db.pool).await.unwrap();
    assert_eq!(
        all,
        vec![Tag {
            id,
            name: "hiking".to_string()
        }]
    );
}

#[tokio::test]
async fn get_or_create_tag_returns_the_same_id_for_an_existing_name() {
    let db = TestDb::new().await;
    let first = get_or_create_tag(&db.pool, "hiking").await.unwrap();
    let second = get_or_create_tag(&db.pool, "hiking").await.unwrap();
    assert_eq!(first, second);
    assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn add_trip_tag_links_the_tag_to_the_trip() {
    let db = TestDb::new().await;
    let trip_id = insert_sample_trip(&db.pool).await;
    let tag_id = get_or_create_tag(&db.pool, "hiking").await.unwrap();

    add_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();

    let tags = list_trip_tags(&db.pool, trip_id).await.unwrap();
    assert_eq!(
        tags,
        vec![Tag {
            id: tag_id,
            name: "hiking".to_string()
        }]
    );
}

#[tokio::test]
async fn add_trip_tag_is_idempotent() {
    let db = TestDb::new().await;
    let trip_id = insert_sample_trip(&db.pool).await;
    let tag_id = get_or_create_tag(&db.pool, "hiking").await.unwrap();

    add_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();
    add_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();

    assert_eq!(list_trip_tags(&db.pool, trip_id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn list_trip_tags_is_alphabetical_and_scoped_to_the_trip() {
    let db = TestDb::new().await;
    let a = insert_sample_trip(&db.pool).await;
    let b = insert_sample_trip(&db.pool).await;
    let hiking = get_or_create_tag(&db.pool, "hiking").await.unwrap();
    let alps = get_or_create_tag(&db.pool, "alps").await.unwrap();
    let other = get_or_create_tag(&db.pool, "other").await.unwrap();

    add_trip_tag(&db.pool, a, hiking).await.unwrap();
    add_trip_tag(&db.pool, a, alps).await.unwrap();
    add_trip_tag(&db.pool, b, other).await.unwrap();

    let tags_a = list_trip_tags(&db.pool, a).await.unwrap();
    assert_eq!(
        tags_a.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        vec!["alps", "hiking"]
    );
}

#[tokio::test]
async fn remove_trip_tag_unlinks_but_keeps_the_tag_row() {
    let db = TestDb::new().await;
    let trip_id = insert_sample_trip(&db.pool).await;
    let tag_id = get_or_create_tag(&db.pool, "hiking").await.unwrap();
    add_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();

    let removed = remove_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();
    assert!(removed);

    assert!(list_trip_tags(&db.pool, trip_id).await.unwrap().is_empty());
    // Orphaned tag stays around for reuse/autocomplete (US-33).
    assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn remove_trip_tag_returns_false_when_no_such_link_exists() {
    let db = TestDb::new().await;
    let trip_id = insert_sample_trip(&db.pool).await;
    let tag_id = get_or_create_tag(&db.pool, "hiking").await.unwrap();

    let removed = remove_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();
    assert!(!removed);
}

#[tokio::test]
async fn deleting_a_trip_cascades_to_its_tag_links_but_not_the_tag() {
    let db = TestDb::new().await;
    let trip_id = insert_sample_trip(&db.pool).await;
    let tag_id = get_or_create_tag(&db.pool, "hiking").await.unwrap();
    add_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();

    sqlx::query("DELETE FROM trip WHERE id = ?")
        .bind(trip_id)
        .execute(&db.pool)
        .await
        .unwrap();

    assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn trips_exist_is_true_when_every_id_is_a_real_trip() {
    let db = TestDb::new().await;
    let a = insert_sample_trip(&db.pool).await;
    let b = insert_sample_trip(&db.pool).await;

    assert!(trips_exist(&db.pool, &[a, b]).await.unwrap());
}

#[tokio::test]
async fn trips_exist_is_false_when_any_id_is_unknown() {
    let db = TestDb::new().await;
    let a = insert_sample_trip(&db.pool).await;

    assert!(!trips_exist(&db.pool, &[a, 999]).await.unwrap());
}

#[tokio::test]
async fn bulk_add_trip_tags_applies_every_tag_to_every_trip() {
    let db = TestDb::new().await;
    let a = insert_sample_trip(&db.pool).await;
    let b = insert_sample_trip(&db.pool).await;

    let names = vec!["alps".to_string(), "hiking".to_string()];
    let applied = bulk_add_trip_tags(&db.pool, &[a, b], &names).await.unwrap();
    assert_eq!(applied.len(), 2);

    for trip_id in [a, b] {
        let tags = list_trip_tags(&db.pool, trip_id).await.unwrap();
        assert_eq!(
            tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["alps", "hiking"]
        );
    }
}

#[tokio::test]
async fn bulk_add_trip_tags_creates_new_tags_on_demand() {
    let db = TestDb::new().await;
    let trip_id = insert_sample_trip(&db.pool).await;

    bulk_add_trip_tags(&db.pool, &[trip_id], &["brand-new".to_string()])
        .await
        .unwrap();

    assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn bulk_add_trip_tags_reuses_an_existing_tag_rather_than_duplicating_it() {
    let db = TestDb::new().await;
    let trip_id = insert_sample_trip(&db.pool).await;
    get_or_create_tag(&db.pool, "hiking").await.unwrap();

    bulk_add_trip_tags(&db.pool, &[trip_id], &["hiking".to_string()])
        .await
        .unwrap();

    assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn bulk_add_trip_tags_is_idempotent() {
    let db = TestDb::new().await;
    let trip_id = insert_sample_trip(&db.pool).await;

    let names = vec!["hiking".to_string()];
    bulk_add_trip_tags(&db.pool, &[trip_id], &names)
        .await
        .unwrap();
    bulk_add_trip_tags(&db.pool, &[trip_id], &names)
        .await
        .unwrap();

    assert_eq!(list_trip_tags(&db.pool, trip_id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn bulk_add_trip_tags_deduplicates_a_repeated_name_in_one_call() {
    let db = TestDb::new().await;
    let trip_id = insert_sample_trip(&db.pool).await;

    let names = vec!["hiking".to_string(), "hiking".to_string()];
    let applied = bulk_add_trip_tags(&db.pool, &[trip_id], &names)
        .await
        .unwrap();

    assert_eq!(
        applied,
        vec![Tag {
            id: applied[0].id,
            name: "hiking".to_string()
        }]
    );
}

// ── The Tags screen (US-83) ──────────────────────────────────────────────────

mod tags_page {
    use super::*;
    use crate::models::{TagOverview, TagShare};
    use crate::server::repo::{
        find_tag_ids, get_trip, insert_share, list_active_shares, resolve_share, shared_tag_names,
        NewShare, ShareTarget,
    };
    use time::{Duration, OffsetDateTime};

    fn noon() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap()
    }

    async fn insert_planned_trip(pool: &SqlitePool) -> i64 {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let stats = compute_stats(&track.points);
        let geojson = build_track_geojson(&track.points);
        insert_trip(
            pool,
            &NewTrip {
                name: "A plan",
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

    async fn a_tag_share(
        pool: &SqlitePool,
        token: &str,
        label: Option<&str>,
        tags: &[&str],
        expires_at: Option<OffsetDateTime>,
    ) -> i64 {
        let names: Vec<String> = tags.iter().map(|tag| tag.to_string()).collect();
        let tag_ids = find_tag_ids(pool, &names).await.unwrap().unwrap();
        insert_share(
            pool,
            &NewShare {
                token,
                label,
                target: ShareTarget::Tags(&tag_ids),
                created_at: noon(),
                expires_at,
            },
        )
        .await
        .unwrap()
    }

    fn overview(id: i64, name: &str, trips: i64, recorded: i64) -> TagOverview {
        TagOverview {
            id,
            name: name.to_string(),
            trip_count: trips,
            recorded_trip_count: recorded,
            shares: Vec::new(),
        }
    }

    #[tokio::test]
    async fn us83_every_tag_is_listed_alphabetically_with_its_trip_counts() {
        let db = TestDb::new().await;
        let recorded = insert_sample_trip(&db.pool).await;
        let planned = insert_planned_trip(&db.pool).await;
        let norway = tag(&db.pool, recorded, "norway").await;
        tag(&db.pool, planned, "norway").await;
        let plans = tag(&db.pool, planned, "plans").await;
        let unused = get_or_create_tag(&db.pool, "alps").await.unwrap();

        assert_eq!(
            list_tag_overview(&db.pool, noon()).await.unwrap(),
            vec![
                overview(unused, "alps", 0, 0),
                overview(norway, "norway", 2, 1),
                overview(plans, "plans", 1, 0),
            ]
        );
    }

    #[tokio::test]
    async fn us83_a_tag_lists_the_active_shares_naming_it_newest_first() {
        let db = TestDb::new().await;
        let trip = insert_sample_trip(&db.pool).await;
        tag(&db.pool, trip, "alps").await;
        tag(&db.pool, trip, "norway").await;
        let older = a_tag_share(&db.pool, "older", None, &["norway", "alps"], None).await;
        let newer = a_tag_share(&db.pool, "newer", Some("Summer"), &["alps"], None).await;
        let expired = Some(noon() - Duration::days(1));
        a_tag_share(&db.pool, "expired", None, &["alps"], expired).await;

        let tags = list_tag_overview(&db.pool, noon()).await.unwrap();

        let share = |id, label: Option<&str>, tags: &[&str]| TagShare {
            id,
            label: label.map(str::to_string),
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
        };
        assert_eq!(
            tags[0].shares,
            vec![
                share(newer, Some("Summer"), &["alps"]),
                share(older, None, &["norway", "alps"]),
            ]
        );
        assert_eq!(
            tags[1].shares,
            vec![share(older, None, &["norway", "alps"])]
        );
    }

    #[tokio::test]
    async fn us83_creating_a_tag_makes_one_carrying_no_trips() {
        let db = TestDb::new().await;

        let created = create_tag(&db.pool, "alps").await.unwrap().unwrap();

        assert_eq!(created.name, "alps");
        assert_eq!(
            list_tag_overview(&db.pool, noon()).await.unwrap(),
            vec![overview(created.id, "alps", 0, 0)]
        );
    }

    #[tokio::test]
    async fn us83_an_existing_name_is_not_created_twice() {
        let db = TestDb::new().await;
        get_or_create_tag(&db.pool, "alps").await.unwrap();

        assert_eq!(create_tag(&db.pool, "alps").await.unwrap(), None);
        assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn us83_deleting_a_tag_takes_it_off_its_trips_and_leaves_them_otherwise_alone() {
        let db = TestDb::new().await;
        let trip = insert_sample_trip(&db.pool).await;
        let alps = tag(&db.pool, trip, "alps").await;
        tag(&db.pool, trip, "norway").await;
        let before = get_trip(&db.pool, trip).await.unwrap().unwrap();

        assert!(delete_tag(&db.pool, alps).await.unwrap());

        let names: Vec<String> = list_all_tags(&db.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|tag| tag.name)
            .collect();
        assert_eq!(names, vec!["norway"]);
        let tags = list_trip_tags(&db.pool, trip).await.unwrap();
        assert_eq!(
            tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["norway"]
        );
        assert_eq!(get_trip(&db.pool, trip).await.unwrap().unwrap(), before);
    }

    #[tokio::test]
    async fn us83_deleting_an_unknown_tag_deletes_nothing() {
        let db = TestDb::new().await;
        get_or_create_tag(&db.pool, "alps").await.unwrap();

        assert!(!delete_tag(&db.pool, 999).await.unwrap());
        assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn us83_a_share_naming_other_tags_too_is_narrowed_and_keeps_working() {
        let db = TestDb::new().await;
        let trip = insert_sample_trip(&db.pool).await;
        let alps = tag(&db.pool, trip, "alps").await;
        tag(&db.pool, trip, "norway").await;
        tag(&db.pool, trip, "summer").await;
        let share = a_tag_share(&db.pool, "t", None, &["norway", "alps", "summer"], None).await;

        delete_tag(&db.pool, alps).await.unwrap();

        assert!(resolve_share(&db.pool, "t", noon())
            .await
            .unwrap()
            .is_some());
        assert_eq!(
            shared_tag_names(&db.pool, share).await.unwrap(),
            vec!["norway", "summer"]
        );
    }

    #[tokio::test]
    async fn us83_a_share_naming_only_the_tag_is_stopped() {
        let db = TestDb::new().await;
        let trip = insert_sample_trip(&db.pool).await;
        let alps = tag(&db.pool, trip, "alps").await;
        a_tag_share(&db.pool, "only", None, &["alps"], None).await;
        let expired = Some(noon() - Duration::days(1));
        let gone = a_tag_share(&db.pool, "expired", None, &["alps"], expired).await;

        delete_tag(&db.pool, alps).await.unwrap();

        assert_eq!(resolve_share(&db.pool, "only", noon()).await.unwrap(), None);
        assert!(list_active_shares(&db.pool, noon())
            .await
            .unwrap()
            .is_empty());
        // Its row goes, as stopping it does — the expired one's too.
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM share")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(left, 0, "share {gone} should have gone");
    }

    #[tokio::test]
    async fn us85_a_renamed_tag_keeps_its_trips_and_shares_under_the_new_name() {
        let db = TestDb::new().await;
        let trip = insert_sample_trip(&db.pool).await;
        let alps = tag(&db.pool, trip, "alps").await;
        let share = a_tag_share(&db.pool, "t", None, &["alps"], None).await;

        let renamed = rename_tag(&db.pool, alps, "alpen").await.unwrap();

        let alpen = Tag {
            id: alps,
            name: "alpen".to_string(),
        };
        assert_eq!(renamed, Rename::Renamed(alpen.clone()));
        assert_eq!(list_trip_tags(&db.pool, trip).await.unwrap(), vec![alpen]);
        assert_eq!(
            shared_tag_names(&db.pool, share).await.unwrap(),
            vec!["alpen"]
        );
    }

    #[tokio::test]
    async fn us85_a_name_another_tag_has_is_taken_and_nothing_changes() {
        let db = TestDb::new().await;
        let alps = get_or_create_tag(&db.pool, "alps").await.unwrap();
        get_or_create_tag(&db.pool, "norway").await.unwrap();

        assert_eq!(
            rename_tag(&db.pool, alps, "norway").await.unwrap(),
            Rename::Taken
        );
        let names: Vec<String> = list_all_tags(&db.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|tag| tag.name)
            .collect();
        assert_eq!(names, vec!["alps", "norway"]);
    }

    #[tokio::test]
    async fn us85_renaming_to_its_own_name_changes_nothing_and_an_unknown_tag_is_missing() {
        let db = TestDb::new().await;
        let alps = get_or_create_tag(&db.pool, "alps").await.unwrap();

        assert_eq!(
            rename_tag(&db.pool, alps, "alps").await.unwrap(),
            Rename::Renamed(Tag {
                id: alps,
                name: "alps".to_string()
            })
        );
        assert_eq!(
            rename_tag(&db.pool, 999, "alpen").await.unwrap(),
            Rename::Missing
        );
        assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
    }
}
