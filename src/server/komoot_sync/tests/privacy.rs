//! Komoot privacy (`status`) sync (ADR-0021): the push phase carries the
//! owner's chosen privacy alongside name/sport, and the pull phase refreshes
//! every linked trip's stored privacy from the tour listing it already
//! fetched. Split out of the parent `tests.rs` to keep that file under the
//! repo's 500-line cap.

use super::*;
use crate::models::KomootPrivacy;
use mock::a_tour_with_status;

/// A linked trip whose privacy the owner changed — the queued push the sync
/// is expected to carry. Goes through the real `repo::update_trip`, so the
/// link row ends up `edit_pending` exactly as a real edit would leave it.
async fn a_pending_privacy_edit(pool: &SqlitePool, tour_id: &str, privacy: KomootPrivacy) -> i64 {
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

    repo::update_trip(
        pool,
        trip_id,
        &repo::TripEdit {
            privacy: Some(privacy),
            ..repo::TripEdit::default()
        },
    )
    .await
    .unwrap();
    trip_id
}

/// A linked trip with a queued *name* edit and no privacy ever stored — the
/// "edit that isn't about privacy" case.
async fn a_pending_name_edit(pool: &SqlitePool, tour_id: &str) -> i64 {
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

    repo::update_trip(pool, trip_id, &repo::TripEdit::named("New Name"))
        .await
        .unwrap();
    trip_id
}

/// A trip of `kind`, linked to `tour_id`, with no privacy stored yet — the
/// state every link row created before privacy mirroring existed is in.
async fn a_linked_trip(pool: &SqlitePool, tour_id: &str, kind: TripKind) -> i64 {
    let trip_id = crate::server::repo::insert_trip(
        pool,
        &crate::server::repo::NewTrip {
            name: "Existing",
            activity_type: ActivityType::Hiking,
            tz_name: "UTC",
            stats: &gpx::compute_stats(&[]),
            geojson: "{}",
            gpx: b"x",
            trip_kind: kind,
        },
    )
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    repo::komoot::insert_link_in_tx(&mut tx, trip_id, tour_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    trip_id
}

/// A linked trip's stored privacy.
async fn stored_privacy(pool: &SqlitePool, tour_id: &str) -> Option<KomootPrivacy> {
    sqlx::query_scalar("SELECT privacy_status FROM trip_komoot_link WHERE komoot_tour_id = ?")
        .bind(tour_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ── push phase ─────────────────────────────────────────────────────────

#[tokio::test]
async fn push_pending_edits_sends_the_chosen_privacy_as_status() {
    let db = TestDb::new().await;
    let trip_id = a_pending_privacy_edit(&db.pool, "555", KomootPrivacy::Public).await;

    let mock = Arc::new(MockKomootClient {
        tour_details: HashMap::from([(
            "555".to_string(),
            a_tour_with_status("555", "irrelevant", "hike", "private"),
        )]),
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
            "Original Name".to_string(),
            "hike".to_string(),
            Some("public".to_string())
        )]
    );
}

#[tokio::test]
async fn push_pending_edits_omits_status_when_no_privacy_is_stored() {
    // A name-only edit on a trip whose privacy was never read or chosen must
    // leave Komoot's own privacy alone rather than guessing at a value.
    let db = TestDb::new().await;
    a_pending_name_edit(&db.pool, "555").await;

    let mock = Arc::new(MockKomootClient {
        tour_details: HashMap::from([("555".to_string(), a_tour("555", "irrelevant", "hike"))]),
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = mock.clone();

    push_pending_edits(&db.pool, client).await.unwrap();

    let calls = mock.update_tour_calls.lock().unwrap();
    assert_eq!(calls[0].3, None, "status must not be sent at all");
}

#[tokio::test]
async fn push_pending_edits_omits_a_privacy_komoot_reported_that_this_app_cannot_map() {
    // ADR-0021: an `unknown` privacy is displayed but never pushed — pushing
    // it would overwrite a real Komoot state this app doesn't understand.
    let db = TestDb::new().await;
    a_pending_privacy_edit(&db.pool, "555", KomootPrivacy::Unknown).await;

    let mock = Arc::new(MockKomootClient {
        tour_details: HashMap::from([("555".to_string(), a_tour("555", "irrelevant", "hike"))]),
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = mock.clone();

    push_pending_edits(&db.pool, client).await.unwrap();

    let calls = mock.update_tour_calls.lock().unwrap();
    assert_eq!(calls[0].3, None);
}

#[tokio::test]
async fn push_pending_edits_sends_privacy_for_a_planned_route_too() {
    // Only `sport` is protected for planned routes; privacy pushes normally.
    let db = TestDb::new().await;
    let trip_id = crate::server::repo::insert_trip(
        &db.pool,
        &crate::server::repo::NewTrip {
            name: "Planned Loop",
            activity_type: ActivityType::Hiking,
            tz_name: "UTC",
            stats: &gpx::compute_stats(&[]),
            geojson: "{}",
            gpx: b"x",
            trip_kind: TripKind::Planned,
        },
    )
    .await
    .unwrap();
    let mut tx = db.pool.begin().await.unwrap();
    repo::komoot::insert_link_in_tx(&mut tx, trip_id, "777")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    repo::update_trip(
        &db.pool,
        trip_id,
        &repo::TripEdit {
            privacy: Some(KomootPrivacy::Private),
            ..repo::TripEdit::default()
        },
    )
    .await
    .unwrap();

    let mock = Arc::new(MockKomootClient {
        tour_details: HashMap::from([(
            "777".to_string(),
            a_tour_with_status("777", "irrelevant", "mtb", "public"),
        )]),
        ..Default::default()
    });
    let client: Arc<dyn KomootClient> = mock.clone();

    push_pending_edits(&db.pool, client).await.unwrap();

    let calls = mock.update_tour_calls.lock().unwrap();
    assert_eq!(calls[0].2, "mtb", "planned sport still resent unchanged");
    assert_eq!(calls[0].3, Some("private".to_string()));
}

// ── pull phase: refresh from the listing ───────────────────────────────

#[tokio::test]
async fn sync_stores_the_privacy_of_a_newly_imported_tour() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    let mut gpx = HashMap::new();
    gpx.insert("999".to_string(), SAMPLE_GPX.to_vec());

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![a_tour_with_status("999", "Mountain Loop", "mtb", "public")],
        gpx,
        ..Default::default()
    });

    let summary = sync_selected_tours(&db.pool, &store, client, &recorded_sel(&["999"]))
        .await
        .unwrap();

    assert!(summary.failed.is_none());
    assert_eq!(
        stored_privacy(&db.pool, "999").await,
        Some(KomootPrivacy::Public)
    );
}

#[tokio::test]
async fn sync_refreshes_an_already_linked_trips_privacy_from_the_listing() {
    // The owner flipped the tour to public inside Komoot itself. The listing
    // the pull already fetches carries that, so the archive picks it up
    // without a single extra API call.
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    let trip_id = crate::server::repo::insert_trip(
        &db.pool,
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
    let mut tx = db.pool.begin().await.unwrap();
    repo::komoot::insert_link_in_tx(&mut tx, trip_id, "111")
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let mut gpx = HashMap::new();
    gpx.insert("222".to_string(), SAMPLE_GPX.to_vec());
    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![
            a_tour_with_status("111", "Already synced", "hike", "public"),
            a_tour_with_status("222", "New", "mtb", "private"),
        ],
        gpx,
        ..Default::default()
    });

    sync_selected_tours(&db.pool, &store, client, &recorded_sel(&["222"]))
        .await
        .unwrap();

    assert_eq!(
        stored_privacy(&db.pool, "111").await,
        Some(KomootPrivacy::Public),
        "an already-linked tour's privacy must be refreshed from the listing"
    );
}

#[tokio::test]
async fn sync_maps_a_privacy_komoot_reports_that_this_app_does_not_know_to_unknown() {
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    let mut gpx = HashMap::new();
    gpx.insert("999".to_string(), SAMPLE_GPX.to_vec());

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![a_tour_with_status(
            "999",
            "Mountain Loop",
            "mtb",
            "friends_only",
        )],
        gpx,
        ..Default::default()
    });

    let summary = sync_selected_tours(&db.pool, &store, client, &recorded_sel(&["999"]))
        .await
        .unwrap();

    assert!(
        summary.failed.is_none(),
        "an unmapped privacy must never fail the sync"
    );
    assert_eq!(
        stored_privacy(&db.pool, "999").await,
        Some(KomootPrivacy::Unknown)
    );
}

#[tokio::test]
async fn the_review_page_listing_refreshes_every_linked_trips_privacy() {
    // A caught-up archive never imports anything, so the pull's own refresh
    // never runs — `sync_selected_tours` only lists the kinds the selection
    // spans, and an empty selection lists nothing. Rendering the review page
    // lists *both* kinds regardless (it has to, to find candidates), so that
    // listing is where a fully-synced archive picks up a privacy changed
    // inside Komoot — still without one extra API call.
    let db = TestDb::new().await;
    let recorded_trip = a_linked_trip(&db.pool, "111", TripKind::Recorded).await;
    let planned_trip = a_linked_trip(&db.pool, "777", TripKind::Planned).await;
    assert!(recorded_trip > 0 && planned_trip > 0);

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![a_tour_with_status("111", "Recorded", "hike", "public")],
        planned_tours: vec![a_tour_with_status("777", "Planned", "hike", "public")],
        ..Default::default()
    });

    let candidates = list_sync_candidates(&db.pool, client).await.unwrap();

    assert!(candidates.is_empty(), "both tours are already linked");
    assert_eq!(
        stored_privacy(&db.pool, "111").await,
        Some(KomootPrivacy::Public)
    );
    assert_eq!(
        stored_privacy(&db.pool, "777").await,
        Some(KomootPrivacy::Public),
        "planned routes refresh from the same listing pass"
    );
}

#[tokio::test]
async fn the_review_page_listing_does_not_overwrite_a_pending_privacy_edit() {
    // Same guard as the pull's refresh: the review page renders *before* the
    // owner presses "Sync now", so a queued edit is still unpushed here.
    let db = TestDb::new().await;
    a_pending_privacy_edit(&db.pool, "111", KomootPrivacy::Public).await;

    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![a_tour_with_status("111", "Pending edit", "hike", "private")],
        ..Default::default()
    });

    list_sync_candidates(&db.pool, client).await.unwrap();

    assert_eq!(
        stored_privacy(&db.pool, "111").await,
        Some(KomootPrivacy::Public)
    );
}

#[tokio::test]
async fn sync_does_not_refresh_a_trip_whose_privacy_edit_is_still_pending() {
    // The push phase runs first; if it halted (leaving the edit pending), the
    // pull must not revert the owner's queued choice to Komoot's stale value.
    let db = TestDb::new().await;
    let (store, _dir) = test_store();
    a_pending_privacy_edit(&db.pool, "111", KomootPrivacy::Public).await;

    let mut gpx = HashMap::new();
    gpx.insert("222".to_string(), SAMPLE_GPX.to_vec());
    let client: Arc<dyn KomootClient> = Arc::new(MockKomootClient {
        tours: vec![
            a_tour_with_status("111", "Pending edit", "hike", "private"),
            a_tour_with_status("222", "New", "mtb", "private"),
        ],
        gpx,
        ..Default::default()
    });

    sync_selected_tours(&db.pool, &store, client, &recorded_sel(&["222"]))
        .await
        .unwrap();

    assert_eq!(
        stored_privacy(&db.pool, "111").await,
        Some(KomootPrivacy::Public),
        "a queued privacy edit outranks the listing's stale value"
    );
}
