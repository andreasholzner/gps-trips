//! `push_pending_edits` (US-20, ADR-0021): push a Komoot-sourced trip's
//! current name/activity_type back to Komoot and clear `edit_pending` on
//! success. Split out of the parent `tests.rs` purely to keep that file
//! under the repo's 500-line cap.

use super::*;

/// Insert a trip, link it to `tour_id`, then edit it through the real
/// `repo::update_trip` (US-15/US-20) so its link row ends up `edit_pending`
/// exactly the way a real owner edit would set it — not a hand-crafted `UPDATE`.
async fn a_pending_edit(
    pool: &SqlitePool,
    tour_id: &str,
    name: &str,
    activity: ActivityType,
) -> i64 {
    let trip_id = crate::server::repo::insert_trip(
        pool,
        "Original Name",
        ActivityType::Hiking,
        "UTC",
        &gpx::compute_stats(&[]),
        "{}",
        b"x",
    )
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    repo::komoot::insert_link_in_tx(&mut tx, trip_id, tour_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    repo::update_trip(pool, trip_id, Some(name), Some(activity))
        .await
        .unwrap();
    trip_id
}

#[tokio::test]
async fn push_pending_edits_reuses_the_live_sport_when_activity_type_is_unchanged() {
    let db = TestDb::new().await;
    // Trip was pulled as `mtb` (-> Cycling locally); the edit only touches
    // the name, so the push must not downgrade Komoot's sport to the
    // generic `touringbicycle` `activity_to_sport(Cycling)` would return.
    let trip_id = a_pending_edit(&db.pool, "555", "New Name", ActivityType::Cycling).await;

    let mock = Arc::new(MockKomootClient {
        tour_details: HashMap::from([("555".to_string(), a_tour("555", "irrelevant", "mtb"))]),
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = mock.clone();

    let summary = push_pending_edits(&db.pool, client).await.unwrap();

    assert!(summary.failed.is_none());
    assert_eq!(summary.pushed, vec![("555".to_string(), trip_id)]);
    let calls = mock.update_tour_calls.lock().unwrap();
    assert_eq!(
        *calls,
        vec![("555".to_string(), "New Name".to_string(), "mtb".to_string())]
    );
}

#[tokio::test]
async fn push_pending_edits_remaps_the_sport_when_activity_type_actually_changed() {
    let db = TestDb::new().await;
    // Trip's current activity_type (Hiking) disagrees with Komoot's live
    // sport (`mtb` -> Cycling) — the owner actually changed it, so the push
    // must send the mapped sport for Hiking, not the stale live one.
    let trip_id = a_pending_edit(&db.pool, "555", "New Name", ActivityType::Hiking).await;

    let mock = Arc::new(MockKomootClient {
        tour_details: HashMap::from([("555".to_string(), a_tour("555", "irrelevant", "mtb"))]),
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = mock.clone();

    let summary = push_pending_edits(&db.pool, client).await.unwrap();

    assert!(summary.failed.is_none());
    assert_eq!(summary.pushed, vec![("555".to_string(), trip_id)]);
    let calls = mock.update_tour_calls.lock().unwrap();
    assert_eq!(
        *calls,
        vec![(
            "555".to_string(),
            "New Name".to_string(),
            "hike".to_string()
        )]
    );
}

#[tokio::test]
async fn push_pending_edits_clears_the_flag_only_on_success() {
    let db = TestDb::new().await;
    let trip_id = a_pending_edit(&db.pool, "555", "New Name", ActivityType::Hiking).await;

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tour_details: HashMap::from([("555".to_string(), a_tour("555", "irrelevant", "hike"))]),
        ..Default::default()
    });

    push_pending_edits(&db.pool, client).await.unwrap();

    assert_eq!(repo::komoot::count_edit_pending(&db.pool).await.unwrap(), 0);
    let _ = trip_id;
}

#[tokio::test]
async fn push_pending_edits_leaves_the_flag_set_when_the_push_fails() {
    let db = TestDb::new().await;
    let trip_id = a_pending_edit(&db.pool, "555", "New Name", ActivityType::Hiking).await;

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tour_details: HashMap::from([("555".to_string(), a_tour("555", "irrelevant", "hike"))]),
        fail_update_tour_for: HashSet::from(["555".to_string()]),
        ..Default::default()
    });

    let summary = push_pending_edits(&db.pool, client).await.unwrap();

    assert!(summary.pushed.is_empty());
    let (failed_tour, _msg) = summary.failed.expect("push must fail");
    assert_eq!(failed_tour, "555");
    assert_eq!(repo::komoot::count_edit_pending(&db.pool).await.unwrap(), 1);
    let _ = trip_id;
}

#[tokio::test]
async fn push_pending_edits_halts_after_the_first_failure_leaving_later_edits_pending() {
    let db = TestDb::new().await;
    let first = a_pending_edit(&db.pool, "111", "First", ActivityType::Hiking).await;
    let second = a_pending_edit(&db.pool, "222", "Second", ActivityType::Hiking).await;

    // Both tours are configured to fail, not just one: `list_edit_pending`
    // has no `ORDER BY`, so which of the two is attempted first is not
    // guaranteed. Failing both makes the assertions below true regardless
    // of attempt order, instead of relying on "111" being processed before
    // "222".
    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tour_details: HashMap::from([
            ("111".to_string(), a_tour("111", "irrelevant", "hike")),
            ("222".to_string(), a_tour("222", "irrelevant", "hike")),
        ]),
        fail_update_tour_for: HashSet::from(["111".to_string(), "222".to_string()]),
        ..Default::default()
    });

    let summary = push_pending_edits(&db.pool, client).await.unwrap();

    assert!(summary.pushed.is_empty());
    let (failed_tour, _msg) = summary
        .failed
        .expect("the first attempted pending edit must fail");
    assert!(
        failed_tour == "111" || failed_tour == "222",
        "unexpected failed tour: {failed_tour}"
    );

    // Both trips must still show as pending — the run halted before even
    // attempting the second one (whichever was attempted first).
    let pending = repo::komoot::list_edit_pending(&db.pool).await.unwrap();
    let pending_trip_ids: HashSet<i64> = pending.iter().map(|p| p.trip_id).collect();
    assert_eq!(pending_trip_ids, HashSet::from([first, second]));
}

#[tokio::test]
async fn push_pending_edits_is_a_noop_with_no_pending_edits() {
    let db = TestDb::new().await;
    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient::default());

    let summary = push_pending_edits(&db.pool, client).await.unwrap();

    assert!(summary.pushed.is_empty());
    assert!(summary.failed.is_none());
}
