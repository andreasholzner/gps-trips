//! `push_pending_deletes` (US-24, ADR-0021): call Komoot's delete-tour API
//! for every `trip_komoot_link` row marked `delete_pending`, and remove the
//! link row on success. Split out of the parent `tests.rs` purely to keep
//! that file under the repo's 500-line cap.

use super::*;

/// Insert a trip, link it to `tour_id`, then delete it through the real
/// `repo::delete_trip` (US-9/US-24) so its link row ends up orphaned and
/// `delete_pending` exactly the way a real owner delete would set it — not a
/// hand-crafted `UPDATE`.
async fn a_pending_delete(pool: &SqlitePool, tour_id: &str) {
    let trip_id = crate::server::repo::insert_trip(
        pool,
        &crate::server::repo::NewTrip {
            name: "Original Name",
            activity_type: ActivityType::Hiking,
            tz_name: "UTC",
            stats: &gpx::compute_stats(&[]),
            geojson: "{}",
            gpx: b"x",
            trip_kind: TripKind::Recorded,
        },
    )
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    repo::komoot::insert_link_in_tx(&mut tx, trip_id, tour_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    crate::server::repo::delete_trip(pool, trip_id)
        .await
        .unwrap();
}

#[tokio::test]
async fn push_pending_deletes_calls_delete_tour_and_removes_the_link_row_on_success() {
    let db = TestDb::new().await;
    a_pending_delete(&db.pool, "555").await;

    let mock = Arc::new(MockKomootClient::default());
    let client: Arc<dyn KomootClient> = mock.clone();

    let summary = push_pending_deletes(&db.pool, client).await.unwrap();

    assert!(summary.failed.is_none());
    assert_eq!(summary.deleted, vec!["555".to_string()]);
    assert_eq!(
        *mock.delete_tour_calls.lock().unwrap(),
        vec!["555".to_string()]
    );
    assert!(repo::komoot::list_delete_pending(&db.pool)
        .await
        .unwrap()
        .is_empty());
    assert!(repo::komoot::list_linked_tour_ids(&db.pool)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn push_pending_deletes_leaves_the_link_row_when_the_delete_fails() {
    let db = TestDb::new().await;
    a_pending_delete(&db.pool, "555").await;

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        fail_delete_tour_for: HashSet::from(["555".to_string()]),
        ..Default::default()
    });

    let summary = push_pending_deletes(&db.pool, client).await.unwrap();

    assert!(summary.deleted.is_empty());
    let (failed_tour, msg) = summary.failed.expect("push must fail");
    assert_eq!(failed_tour, "555");
    assert!(
        msg.contains("delete tour"),
        "failure message must be traceable to a delete failure: {msg}"
    );
    assert_eq!(
        repo::komoot::list_delete_pending(&db.pool).await.unwrap(),
        vec!["555".to_string()]
    );
}

#[tokio::test]
async fn push_pending_deletes_halts_after_the_first_failure_leaving_later_deletes_pending() {
    let db = TestDb::new().await;
    a_pending_delete(&db.pool, "111").await;
    a_pending_delete(&db.pool, "222").await;

    // Both tours are configured to fail, not just one: `list_delete_pending`
    // has no `ORDER BY`, so which of the two is attempted first is not
    // guaranteed. Failing both makes the assertions below true regardless
    // of attempt order, instead of relying on "111" being processed before
    // "222".
    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        fail_delete_tour_for: HashSet::from(["111".to_string(), "222".to_string()]),
        ..Default::default()
    });

    let summary = push_pending_deletes(&db.pool, client).await.unwrap();

    assert!(summary.deleted.is_empty());
    let (failed_tour, _msg) = summary
        .failed
        .expect("the first attempted pending delete must fail");
    assert!(
        failed_tour == "111" || failed_tour == "222",
        "unexpected failed tour: {failed_tour}"
    );

    // Both tours must still show as pending — the run halted before even
    // attempting the second one (whichever was attempted first).
    let pending: HashSet<String> = repo::komoot::list_delete_pending(&db.pool)
        .await
        .unwrap()
        .into_iter()
        .collect();
    assert_eq!(
        pending,
        HashSet::from(["111".to_string(), "222".to_string()])
    );
}

#[tokio::test]
async fn push_pending_deletes_is_a_noop_with_no_pending_deletes() {
    let db = TestDb::new().await;
    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient::default());

    let summary = push_pending_deletes(&db.pool, client).await.unwrap();

    assert!(summary.deleted.is_empty());
    assert!(summary.failed.is_none());
}
