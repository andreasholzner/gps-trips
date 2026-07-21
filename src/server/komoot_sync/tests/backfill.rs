//! `backfill` (US-23, ADR-0021): historical bulk import. Split out of the
//! parent `tests.rs` purely to keep that file under the repo's 500-line cap
//! (mirrors `tests/push.rs`/`tests/delete.rs`).

use super::*;

/// Insert a trip and link it to `tour_id`, so it counts as already synced.
async fn a_linked_trip(pool: &SqlitePool, tour_id: &str) {
    let trip_id = crate::server::repo::insert_trip(
        pool,
        &crate::server::repo::NewTrip {
            name: "Existing",
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
}

fn gpx_for(tour_ids: &[&str]) -> HashMap<String, Vec<u8>> {
    tour_ids
        .iter()
        .map(|id| (id.to_string(), SAMPLE_GPX.to_vec()))
        .collect()
}

#[tokio::test]
async fn backfill_without_a_limit_imports_every_not_yet_linked_tour() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![
            a_tour("111", "First", "hike"),
            a_tour("222", "Second", "mtb"),
            a_tour("333", "Third", "hike"),
        ],
        gpx: gpx_for(&["111", "222", "333"]),
        ..Default::default()
    });

    let summary = backfill(&db.pool, &store, client, None, TripKind::Recorded)
        .await
        .unwrap();

    assert!(summary.failed.is_none());
    assert_eq!(summary.imported.len(), 3);
}

#[tokio::test]
async fn backfill_with_a_limit_imports_only_that_many_tours() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![
            a_tour("111", "First", "hike"),
            a_tour("222", "Second", "mtb"),
            a_tour("333", "Third", "hike"),
        ],
        gpx: gpx_for(&["111", "222", "333"]),
        ..Default::default()
    });

    let summary = backfill(&db.pool, &store, client, Some(2), TripKind::Recorded)
        .await
        .unwrap();

    assert!(summary.failed.is_none());
    assert_eq!(summary.imported.len(), 2);
}

#[tokio::test]
async fn backfill_never_re_imports_an_already_linked_tour() {
    // Simulates a rerun after an earlier backfill already linked "111".
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    a_linked_trip(&db.pool, "111").await;

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![
            a_tour("111", "Already synced", "hike"),
            a_tour("222", "New", "mtb"),
        ],
        gpx: gpx_for(&["222"]),
        ..Default::default()
    });

    let summary = backfill(&db.pool, &store, client, None, TripKind::Recorded)
        .await
        .unwrap();

    assert!(summary.failed.is_none());
    assert_eq!(summary.imported.len(), 1);
    assert_eq!(summary.imported[0].0, "222");
}

#[tokio::test]
async fn backfill_halts_on_the_first_failure_and_never_attempts_the_next_tour() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![
            a_tour("111", "Fails", "hike"),
            a_tour("222", "Never attempted", "hike"),
        ],
        gpx: gpx_for(&["222"]), // "111" has no gpx fixture -> its fetch fails
        fail_gpx_for: HashSet::from(["111".to_string()]),
        ..Default::default()
    });

    let summary = backfill(&db.pool, &store, client, None, TripKind::Recorded)
        .await
        .unwrap();

    assert!(summary.imported.is_empty());
    let (failed_tour, _msg) = summary.failed.expect("first tour must fail");
    assert_eq!(failed_tour, "111");
}

#[tokio::test]
async fn backfill_logs_in_and_lists_tours_only_once_per_run() {
    // ADR-0021: "KomootClient still logs in once per sync invocation... to
    // resolve the Komoot username and validate credentials up front" — a
    // single backfill() call is one invocation, so it must not pay for a
    // second login/listing pass just because it's internally composed of
    // two steps (finding candidates, then importing them).
    let db = TestDb::new().await;
    let (store, _dir) = test_store();

    let mock = Arc::new(MockKomootClient {
        tours: vec![
            a_tour("111", "First", "hike"),
            a_tour("222", "Second", "mtb"),
        ],
        gpx: gpx_for(&["111", "222"]),
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = Arc::clone(&mock) as Arc<dyn KomootClient>;

    let summary = backfill(&db.pool, &store, client, None, TripKind::Recorded)
        .await
        .unwrap();
    assert!(summary.failed.is_none());
    assert_eq!(summary.imported.len(), 2);

    assert_eq!(
        *mock.login_calls.lock().unwrap(),
        1,
        "backfill must log in exactly once per run"
    );
    assert_eq!(
        mock.list_tours_calls.lock().unwrap().as_slice(),
        &[0u32],
        "backfill must list Komoot's tours only once per run (one page 0 \
         request; the mock returns a short page so no page 1 follows)"
    );
}

#[tokio::test]
async fn backfill_planned_imports_planned_routes_and_never_touches_recorded() {
    // US-29: `--planned` pulls the planned endpoint only — a recorded tour
    // configured on the same mock must be neither imported nor even listed.
    let db = TestDb::new().await;
    let (store, _dir) = test_store();

    let mock = Arc::new(MockKomootClient {
        tours: vec![a_tour("111", "Recorded", "hike")],
        planned_tours: vec![a_tour("777", "Planned", "hike")],
        gpx: gpx_for(&["111", "777"]),
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = Arc::clone(&mock) as Arc<dyn KomootClient>;

    let summary = backfill(&db.pool, &store, client, None, TripKind::Planned)
        .await
        .unwrap();

    assert!(summary.failed.is_none());
    assert_eq!(summary.imported.len(), 1);
    let (tour_id, trip_id) = &summary.imported[0];
    assert_eq!(tour_id, "777");

    let kind: TripKind = sqlx::query_scalar("SELECT trip_kind FROM trip WHERE id = ?")
        .bind(*trip_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(kind, TripKind::Planned);

    assert!(
        mock.list_tours_calls.lock().unwrap().is_empty(),
        "a planned backfill must not list the recorded endpoint"
    );
    assert_eq!(
        mock.list_planned_tours_calls.lock().unwrap().as_slice(),
        &[0u32],
        "a planned backfill lists the planned endpoint once"
    );
}

#[tokio::test]
async fn backfill_with_no_candidates_reports_nothing_imported_and_no_failure() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient::default());

    let summary = backfill(&db.pool, &store, client, None, TripKind::Recorded)
        .await
        .unwrap();

    assert!(summary.imported.is_empty());
    assert!(summary.failed.is_none());
}
