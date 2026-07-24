use super::*;

async fn bootstrapped() -> (tempfile::TempDir, SqliteConnection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = create_and_bootstrap(&dir.path().join("target.db"), "target")
        .await
        .expect("bootstrap");
    (dir, conn)
}

async fn insert_dummy_item(conn: &mut SqliteConnection, keyqms: &str, folder: i64) {
    insert_item(
        conn,
        &NewItem {
            keyqms,
            name: "A track",
            icon: b"\x89PNGfake",
            date: "2024-06-01T08:00:00Z",
            comment: "a comment for the search index",
            data: b"blobbytes",
            hash: "00000000000000000000000000000000",
        },
        folder,
    )
    .await
    .expect("insert item");
}

#[tokio::test]
async fn bootstrap_creates_the_full_qmapshack_schema() {
    let (_dir, mut conn) = bootstrapped().await;

    let names: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type IN ('table', 'trigger')")
            .fetch_all(&mut conn)
            .await
            .unwrap();
    for expected in [
        "versioninfo",
        "folders",
        "items",
        "folder2folder",
        "folder2item",
        "searchindex",
        "items_update_last_change",
        "folder2item_insert",
        "folder2item_delete",
        "searchindex_update",
        "searchindex_insert",
    ] {
        assert!(names.iter().any(|n| n == expected), "missing {expected}");
    }

    let (version, db_type): (String, String) =
        sqlx::query_as("SELECT version, type FROM versioninfo")
            .fetch_one(&mut conn)
            .await
            .unwrap();
    assert_eq!((version.as_str(), db_type.as_str()), ("6", "QMapShack"));

    let (root_type, root_name): (i64, String) = sqlx::query_as("SELECT type, name FROM folders")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(root_type, 2);
    assert_eq!(root_name, "target");
    assert!(root_folder_id(&mut conn).await.is_ok());
}

#[tokio::test]
async fn version_gate_accepts_only_the_targeted_version() {
    let (_dir, mut conn) = bootstrapped().await;
    check_version(&mut conn)
        .await
        .expect("bootstrapped DB passes");

    sqlx::query("UPDATE versioninfo SET version = '7'")
        .execute(&mut conn)
        .await
        .unwrap();
    let err = check_version(&mut conn).await.expect_err("newer version");
    assert!(matches!(
        err,
        TargetError::VersionMismatch { ref found } if found == "7"
    ));
    assert!(err.to_string().contains('7'), "names the found version");
    assert!(err.to_string().contains('6'), "names the expected version");
}

#[tokio::test]
async fn version_gate_propagates_a_locked_database_as_a_sql_error() {
    // The owner still has the file open in QMapShack (write lock held):
    // that must surface as a real SQL error, not as the misleading
    // "not a QMapShack database".
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("target.db");
    drop(create_and_bootstrap(&path, "target").await.unwrap());

    // Reader with no busy grace so the test doesn't sit out a timeout.
    let mut reader = connect_options(&path, false)
        .busy_timeout(Duration::ZERO)
        .connect()
        .await
        .unwrap();
    let mut writer = open_existing(&path).await.unwrap();
    sqlx::query("BEGIN EXCLUSIVE")
        .execute(&mut writer)
        .await
        .expect("take the write lock");

    let err = check_version(&mut reader).await.expect_err("locked file");
    assert!(
        matches!(err, TargetError::Sql(_)),
        "a locked database is a SQL error, got: {err}"
    );
}

#[tokio::test]
async fn version_gate_rejects_a_non_qmapshack_database() {
    let dir = tempfile::tempdir().unwrap();
    // A plain SQLite file with no versioninfo table.
    let mut conn = connect_options(&dir.path().join("random.db"), true)
        .connect()
        .await
        .unwrap();
    sqlx::query("CREATE TABLE unrelated (x)")
        .execute(&mut conn)
        .await
        .unwrap();
    let err = check_version(&mut conn).await.expect_err("not QMapShack");
    assert!(matches!(err, TargetError::NotAQmapShackDb));
}

#[tokio::test]
async fn open_existing_fails_for_a_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let err = open_existing(&dir.path().join("nope.db"))
        .await
        .expect_err("missing file");
    assert!(matches!(err, TargetError::Open { .. }));
}

#[tokio::test]
async fn triggers_maintain_search_index_and_trash() {
    let (_dir, mut conn) = bootstrapped().await;
    let root = root_folder_id(&mut conn).await.unwrap();
    let folder = ensure_folder_path(&mut conn, root, &["Trips".to_string()])
        .await
        .unwrap();
    insert_dummy_item(&mut conn, "trip-archive:trip:1", folder).await;

    // searchindex_insert fired — this is also the runtime proof that
    // sqlx's bundled SQLite has FTS4 compiled in.
    let indexed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM searchindex WHERE comment MATCH 'search'")
            .fetch_one(&mut conn)
            .await
            .unwrap();
    assert_eq!(indexed, 1);

    // Unlinking the item from its last folder moves it to the trash.
    let trash_before: Option<String> = sqlx::query_scalar("SELECT trash FROM items")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(trash_before, None);
    sqlx::query("DELETE FROM folder2item")
        .execute(&mut conn)
        .await
        .unwrap();
    let trash_after: Option<String> = sqlx::query_scalar("SELECT trash FROM items")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert!(trash_after.is_some(), "folder2item_delete sets trash");
}

#[tokio::test]
async fn ensure_folder_path_creates_groups_then_a_project_leaf() {
    let (_dir, mut conn) = bootstrapped().await;
    let root = root_folder_id(&mut conn).await.unwrap();
    let segments: Vec<String> = ["Trips", "2024", "Hiking"]
        .into_iter()
        .map(String::from)
        .collect();

    let leaf = ensure_folder_path(&mut conn, root, &segments)
        .await
        .unwrap();

    let types: Vec<(String, i64)> =
        sqlx::query_as("SELECT name, type FROM folders WHERE type != 2 ORDER BY id")
            .fetch_all(&mut conn)
            .await
            .unwrap();
    assert_eq!(
        types,
        [
            ("Trips".to_string(), FOLDER_GROUP),
            ("2024".to_string(), FOLDER_GROUP),
            ("Hiking".to_string(), FOLDER_PROJECT),
        ]
    );

    // Idempotent: a second walk resolves to the same leaf, creating nothing.
    let again = ensure_folder_path(&mut conn, root, &segments)
        .await
        .unwrap();
    assert_eq!(again, leaf);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM folders")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(count, 4, "root + the three created folders");

    // A sibling path shares the existing prefix.
    let sibling: Vec<String> = ["Trips", "2024", "Cycling"]
        .into_iter()
        .map(String::from)
        .collect();
    ensure_folder_path(&mut conn, root, &sibling).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM folders")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(count, 5, "only the new leaf was created");
}

// ── US-37 reconciliation helpers ─────────────────────────────────────────────

/// The item's current folder links, ascending.
async fn links_of(conn: &mut SqliteConnection, id: i64) -> Vec<i64> {
    sqlx::query_scalar("SELECT parent FROM folder2item WHERE child = ? ORDER BY parent")
        .bind(id)
        .fetch_all(conn)
        .await
        .unwrap()
}

async fn trash_of(conn: &mut SqliteConnection, id: i64) -> Option<String> {
    sqlx::query_scalar("SELECT trash FROM items WHERE id = ?")
        .bind(id)
        .fetch_one(conn)
        .await
        .unwrap()
}

#[tokio::test]
async fn get_item_state_returns_columns_and_sorted_links() {
    let (_dir, mut conn) = bootstrapped().await;
    let root = root_folder_id(&mut conn).await.unwrap();
    assert!(get_item_state(&mut conn, "trip-archive:trip:1")
        .await
        .unwrap()
        .is_none());

    insert_dummy_item(&mut conn, "trip-archive:trip:1", root).await;
    let state = get_item_state(&mut conn, "trip-archive:trip:1")
        .await
        .unwrap()
        .expect("item found");
    assert_eq!(state.name, "A track");
    assert_eq!(
        state.comment.as_deref(),
        Some("a comment for the search index")
    );
    assert_eq!(state.folder_ids, [root]);

    // A trashed item still resolves, with no links.
    sqlx::query("DELETE FROM folder2item")
        .execute(&mut conn)
        .await
        .unwrap();
    let state = get_item_state(&mut conn, "trip-archive:trip:1")
        .await
        .unwrap()
        .expect("trashed item still found");
    assert_eq!(state.folder_ids, Vec::<i64>::new());
}

#[tokio::test]
async fn update_item_rewrites_columns_and_the_search_index() {
    let (_dir, mut conn) = bootstrapped().await;
    let root = root_folder_id(&mut conn).await.unwrap();
    insert_dummy_item(&mut conn, "trip-archive:trip:1", root).await;
    let state = get_item_state(&mut conn, "trip-archive:trip:1")
        .await
        .unwrap()
        .unwrap();

    update_item(
        &mut conn,
        state.id,
        &NewItem {
            keyqms: "trip-archive:trip:1",
            name: "Renamed track",
            icon: b"\x89PNGother",
            date: "2025-01-01T10:00:00Z",
            comment: "a fresh comment",
            data: b"newblob",
            hash: "11111111111111111111111111111111",
        },
    )
    .await
    .expect("update item");

    let (name, comment, data, hash): (String, String, Vec<u8>, String) =
        sqlx::query_as("SELECT name, comment, data, hash FROM items WHERE id = ?")
            .bind(state.id)
            .fetch_one(&mut conn)
            .await
            .unwrap();
    assert_eq!(name, "Renamed track");
    assert_eq!(comment, "a fresh comment");
    assert_eq!(data, b"newblob");
    assert_eq!(hash, "11111111111111111111111111111111");

    // searchindex_update followed the comment.
    let indexed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM searchindex WHERE comment MATCH 'fresh'")
            .fetch_one(&mut conn)
            .await
            .unwrap();
    assert_eq!(indexed, 1);
}

#[tokio::test]
async fn set_item_folder_relinks_to_exactly_one_folder_without_trash_flicker() {
    let (_dir, mut conn) = bootstrapped().await;
    let root = root_folder_id(&mut conn).await.unwrap();
    let folder_a = ensure_folder_path(&mut conn, root, &["A".to_string()])
        .await
        .unwrap();
    let folder_b = ensure_folder_path(&mut conn, root, &["B".to_string()])
        .await
        .unwrap();
    insert_dummy_item(&mut conn, "trip-archive:trip:1", folder_a).await;
    let id = get_item_state(&mut conn, "trip-archive:trip:1")
        .await
        .unwrap()
        .unwrap()
        .id;

    // Plain move: A → B.
    set_item_folder(&mut conn, id, folder_b).await.unwrap();
    assert_eq!(links_of(&mut conn, id).await, [folder_b]);
    assert_eq!(trash_of(&mut conn, id).await, None, "no trash flicker");

    // Idempotent: already exactly there.
    set_item_folder(&mut conn, id, folder_b).await.unwrap();
    assert_eq!(links_of(&mut conn, id).await, [folder_b]);

    // Owner added an extra link: reconciled back to exactly one.
    sqlx::query("INSERT INTO folder2item (parent, child) VALUES (?, ?)")
        .bind(folder_a)
        .bind(id)
        .execute(&mut conn)
        .await
        .unwrap();
    set_item_folder(&mut conn, id, folder_b).await.unwrap();
    assert_eq!(links_of(&mut conn, id).await, [folder_b]);

    // Trashed item: re-linking restores it (folder2item_insert trigger).
    sqlx::query("DELETE FROM folder2item WHERE child = ?")
        .bind(id)
        .execute(&mut conn)
        .await
        .unwrap();
    assert!(trash_of(&mut conn, id).await.is_some(), "in the trash");
    set_item_folder(&mut conn, id, folder_a).await.unwrap();
    assert_eq!(links_of(&mut conn, id).await, [folder_a]);
    assert_eq!(trash_of(&mut conn, id).await, None, "restored");

    // Duplicate links to the *desired* folder collapse to one row too —
    // otherwise the placement compare ([a, a] != [a]) churns every run.
    sqlx::query("INSERT INTO folder2item (parent, child) VALUES (?, ?)")
        .bind(folder_a)
        .bind(id)
        .execute(&mut conn)
        .await
        .unwrap();
    assert_eq!(links_of(&mut conn, id).await, [folder_a, folder_a]);
    set_item_folder(&mut conn, id, folder_a).await.unwrap();
    assert_eq!(links_of(&mut conn, id).await, [folder_a]);
    assert_eq!(trash_of(&mut conn, id).await, None, "still not trashed");
}

#[tokio::test]
async fn unlink_item_moves_it_to_the_trash() {
    let (_dir, mut conn) = bootstrapped().await;
    let root = root_folder_id(&mut conn).await.unwrap();
    insert_dummy_item(&mut conn, "trip-archive:trip:1", root).await;
    let id = get_item_state(&mut conn, "trip-archive:trip:1")
        .await
        .unwrap()
        .unwrap()
        .id;

    unlink_item(&mut conn, id).await.unwrap();
    assert_eq!(links_of(&mut conn, id).await, Vec::<i64>::new());
    assert!(trash_of(&mut conn, id).await.is_some(), "trigger set trash");
}

#[tokio::test]
async fn list_exporter_items_scopes_to_the_namespace_prefix() {
    let (_dir, mut conn) = bootstrapped().await;
    let root = root_folder_id(&mut conn).await.unwrap();
    insert_dummy_item(&mut conn, "trip-archive:trip:1", root).await;
    insert_dummy_item(&mut conn, "trip-archive:trip:2", root).await;
    // The owner's own item (QMapShack MD5-style key) is out of scope.
    insert_dummy_item(&mut conn, "a3f5c8d92b1e4f6789abcdef01234567", root).await;

    let trashed_id = get_item_state(&mut conn, "trip-archive:trip:2")
        .await
        .unwrap()
        .unwrap()
        .id;
    unlink_item(&mut conn, trashed_id).await.unwrap();

    let mut items = list_exporter_items(&mut conn, "trip-archive:trip:")
        .await
        .unwrap();
    items.sort_by(|a, b| a.keyqms.cmp(&b.keyqms));
    assert_eq!(items.len(), 2, "foreign item is never even listed");
    assert_eq!(items[0].keyqms, "trip-archive:trip:1");
    assert!(items[0].linked);
    assert_eq!(items[1].keyqms, "trip-archive:trip:2");
    assert!(!items[1].linked, "trashed item reported as unlinked");
}
