use super::*;

#[test]
fn us88_a_file_is_kept_under_its_own_name_without_a_path() {
    assert_eq!(file_name_of(Some("Part 1.gpx")), "Part 1.gpx");
    assert_eq!(file_name_of(Some("C:\\books\\Part 1.gpx")), "Part 1.gpx");
    assert_eq!(file_name_of(Some("../../etc/passwd")), "passwd");
    assert_eq!(file_name_of(Some("  ")), "collection.gpx");
    assert_eq!(file_name_of(None), "collection.gpx");
}

#[test]
fn us88_files_are_taken_in_the_order_of_their_names() {
    let mut files: Vec<(String, Vec<u8>)> = ["b.gpx", "A.gpx", "a.gpx", "C.gpx"]
        .into_iter()
        .map(|name| (name.to_string(), Vec::new()))
        .collect();

    in_name_order(&mut files);

    let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["A.gpx", "a.gpx", "b.gpx", "C.gpx"]);
}

// ── The write lock (review finding: held only to store the outcome) ─────────

const GUIDE_BOOK: &[u8] = include_bytes!("../../../tests/fixtures/guide_book.gpx");

async fn a_state() -> (
    AppState,
    crate::server::db::testing::TestDb,
    tempfile::TempDir,
) {
    use crate::server::{auth::Auth, auth::Salt, storage::LocalDisk};
    let db = crate::server::db::testing::TestDb::new().await;
    let dir = tempfile::tempdir().expect("temp dir");
    let store: std::sync::Arc<dyn crate::server::storage::BlobStore> =
        std::sync::Arc::new(LocalDisk::new(dir.path().join("blobs")));
    let auth = Auth::new("a test password", &Salt::from([7; 16])).expect("a password");
    (AppState::new(db.pool.clone(), store, None, auth), db, dir)
}

#[tokio::test]
async fn us88_the_files_are_matched_without_holding_the_write_lock() {
    let (state, _db, _dir) = a_state().await;
    let files = [NewCollectionFile {
        file_name: "book.gpx",
        gpx: GUIDE_BOOK,
    }];
    let staging_id =
        repo::insert_collection_import(&state.pool, None, &files, OffsetDateTime::now_utc())
            .await
            .unwrap();
    let confirm = ConfirmCollectionImport {
        held_digest: Held::default().digest(),
        ..ConfirmCollectionImport::default()
    };

    // Another writer holds the lock for as long as the matching takes.
    let blocker = db::begin_write(&state.pool).await.unwrap();
    let prepared = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        prepare(&state, staging_id, &confirm),
    )
    .await
    .expect("prepared without waiting for the lock")
    .unwrap();
    blocker.rollback().await.unwrap();

    let id = store(&state, prepared, None).await.unwrap();
    let collection = repo::get_collection(&state.pool, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(collection.tracks.len(), 3);
}

#[tokio::test]
async fn us88_a_collection_changed_while_the_files_were_matched_is_refused() {
    let (state, _db, _dir) = a_state().await;
    let files = [NewCollectionFile {
        file_name: "book.gpx",
        gpx: GUIDE_BOOK,
    }];
    let target = repo::insert_collection(
        &state.pool,
        &repo::NewCollection {
            name: "Lakes",
            description: "",
            activity_type: None,
            files: &files,
            tracks: &[],
            waypoints: &[],
        },
    )
    .await
    .unwrap();
    let staging_id = repo::insert_collection_import(
        &state.pool,
        Some(target),
        &files,
        OffsetDateTime::now_utc(),
    )
    .await
    .unwrap();
    let confirm = ConfirmCollectionImport {
        held_digest: Held::default().digest(),
        ..ConfirmCollectionImport::default()
    };
    let prepared = prepare(&state, staging_id, &confirm).await.unwrap();

    // Between the matching and the storing, the collection gains a track.
    let mut tx = db::begin_write(&state.pool).await.unwrap();
    let stats = crate::server::gpx::compute_stats(&[crate::server::gpx::TrackPoint {
        lat: 46.0,
        lon: 9.0,
        ele: None,
        time: None,
    }]);
    crate::server::repo::apply_collection_import(
        &mut tx,
        target,
        &repo::CollectionImportPlan {
            replace_all: false,
            remove_tracks: &[],
            remove_waypoints: &[],
            tracks: &[repo::PlannedTrack::Insert(repo::NewCollectionTrack {
                name: "Elsewhere",
                stats: &stats,
                geojson: r#"{"type":"Feature","geometry":{"type":"LineString","coordinates":[[9.0,46.0]]}}"#,
                gpx: b"<gpx/>",
            })],
            waypoints: &[],
            files: &[],
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let refused = store(&state, prepared, None).await;

    assert!(matches!(refused, Err(AppError::Conflict(_))), "{refused:?}");
    assert!(repo::get_collection_import(&state.pool, staging_id)
        .await
        .unwrap()
        .is_some());
}
