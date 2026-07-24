//! Access to the *target* QMapShack database (US-36). This is a foreign,
//! reverse-engineered schema — never touched by `db::create_pool` (no
//! trip-archive migrations, no WAL: QMapShack files use the rollback
//! journal, and converting the owner's file would be a destructive
//! surprise). Schema SQL is copied verbatim from QMapShack's
//! `IDBSqlite::initDB()`; its triggers do the trash/FTS bookkeeping, so
//! plain INSERT/DELETE statements are all a writer needs
//! (`docs/qmapshack-format.md`).

use std::path::Path;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use sqlx::{ConnectOptions, Connection, Row, SqliteConnection};

use super::DB_VERSION;

#[derive(Debug, thiserror::Error)]
pub enum TargetError {
    #[error("cannot open target database {path}: {source}")]
    Open { path: String, source: sqlx::Error },
    #[error("not a QMapShack database (it has no versioninfo table); refusing to touch it")]
    NotAQmapShackDb,
    #[error(
        "target database version is {found}, but this exporter targets {DB_VERSION}: \
         open the file in QMapShack to migrate it (older) or update trip-archive \
         (newer); nothing was written"
    )]
    VersionMismatch { found: String },
    #[error("target database has no root folder (type 2)")]
    MissingRootFolder,
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}

fn connect_options(path: &Path, create: bool) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create)
        // sqlx defaults to WAL — QMapShack files must stay on the rollback
        // journal, so pin Delete mode explicitly.
        .journal_mode(SqliteJournalMode::Delete)
        // QMapShack never enables SQLite FK enforcement; mirror that.
        .foreign_keys(false)
        .busy_timeout(Duration::from_secs(5))
}

/// Open an existing target database; fails if the file doesn't exist.
pub async fn open_existing(path: &Path) -> Result<SqliteConnection, TargetError> {
    connect_options(path, false)
        .connect()
        .await
        .map_err(|source| TargetError::Open {
            path: path.display().to_string(),
            source,
        })
}

/// Create a fresh target database with the full QMapShack schema
/// (`IDBSqlite::initDB()` verbatim), a `versioninfo` row for the version
/// this exporter targets, and a root folder named `root_name` (QMapShack
/// uses its connection name there; we use the file stem).
pub async fn create_and_bootstrap(
    path: &Path,
    root_name: &str,
) -> Result<SqliteConnection, TargetError> {
    let mut conn = connect_options(path, true)
        .connect()
        .await
        .map_err(|source| TargetError::Open {
            path: path.display().to_string(),
            source,
        })?;

    let mut tx = conn.begin().await?;
    sqlx::query("CREATE TABLE versioninfo ( version TEXT, type TEXT )")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO versioninfo (version, type) VALUES(?, 'QMapShack')")
        .bind(DB_VERSION)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "CREATE TABLE folders (\
         id             INTEGER PRIMARY KEY AUTOINCREMENT,\
         type           INTEGER NOT NULL,\
         keyqms         TEXT,\
         date           DATETIME DEFAULT CURRENT_TIMESTAMP,\
         name           TEXT NOT NULL,\
         comment        TEXT,\
         locked         BOOLEAN DEFAULT FALSE,\
         data           BLOB,\
         sortmode       INTEGER NOT NULL DEFAULT 0\
         )",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "CREATE TABLE items (\
         id             INTEGER PRIMARY KEY AUTOINCREMENT,\
         type           INTEGER,\
         keyqms         TEXT NOT NULL UNIQUE,\
         date           DATETIME DEFAULT CURRENT_TIMESTAMP,\
         icon           BLOB NOT NULL,\
         name           TEXT NOT NULL,\
         comment        TEXT,\
         data           BLOB NOT NULL,\
         hash           TEXT NOT NULL,\
         last_user      TEXT DEFAULT 'QMapShack',\
         last_change    DATETIME DEFAULT CURRENT_TIMESTAMP,\
         trash          DATETIME DEFAULT NULL\
         )",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "CREATE TRIGGER items_update_last_change \
         AFTER UPDATE ON items BEGIN \
         UPDATE items SET last_change=CURRENT_TIMESTAMP WHERE id=NEW.id; \
         END;",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO folders (type, name, comment) VALUES (2, ?, '')")
        .bind(root_name)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "CREATE TABLE folder2folder (\
         id             INTEGER PRIMARY KEY AUTOINCREMENT,\
         parent         INTEGER NOT NULL,\
         child          INTEGER NOT NULL,\
         FOREIGN KEY(parent) REFERENCES folders(id),\
         FOREIGN KEY(child) REFERENCES folders(id)\
         )",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "CREATE TABLE folder2item (\
         id             INTEGER PRIMARY KEY AUTOINCREMENT,\
         parent         INTEGER NOT NULL,\
         child          INTEGER NOT NULL,\
         FOREIGN KEY(parent) REFERENCES folders(id),\
         FOREIGN KEY(child) REFERENCES items(id)\
         )",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "CREATE TRIGGER folder2item_insert \
         BEFORE INSERT ON folder2item BEGIN \
         UPDATE items SET trash=NULL \
         WHERE id=NEW.child; \
         END;",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "CREATE TRIGGER folder2item_delete \
         AFTER DELETE ON folder2item BEGIN \
         UPDATE items SET trash=CURRENT_TIMESTAMP \
         WHERE id=OLD.child AND OLD.child NOT IN(SELECT child FROM folder2item); \
         END;",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("CREATE VIRTUAL TABLE searchindex USING fts4(id, comment)")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "CREATE TRIGGER searchindex_update \
         AFTER UPDATE ON items BEGIN \
         UPDATE searchindex SET comment=NEW.comment \
         WHERE id=OLD.id; \
         END;",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "CREATE TRIGGER searchindex_insert \
         AFTER INSERT ON items BEGIN \
         INSERT INTO searchindex(id, comment) VALUES(NEW.id, NEW.comment); \
         END;",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(conn)
}

/// The compatibility gate (ADR-0022): refuse to touch anything whose
/// `versioninfo` doesn't exactly match what this exporter was built for.
/// Only a *missing* `versioninfo` table means "not a QMapShack database" —
/// any other failure (e.g. `SQLITE_BUSY` because the owner still has the
/// file open in QMapShack) is a real error and must be reported as one.
pub async fn check_version(conn: &mut SqliteConnection) -> Result<(), TargetError> {
    let row = match sqlx::query("SELECT version, type FROM versioninfo")
        .fetch_optional(&mut *conn)
        .await
    {
        Ok(row) => row,
        Err(e) if is_missing_table(&e) => return Err(TargetError::NotAQmapShackDb),
        Err(e) => return Err(TargetError::Sql(e)),
    };
    let Some(row) = row else {
        return Err(TargetError::NotAQmapShackDb);
    };
    let version: String = row.get("version");
    let db_type: String = row.get("type");
    if db_type != "QMapShack" {
        return Err(TargetError::NotAQmapShackDb);
    }
    if version != DB_VERSION {
        return Err(TargetError::VersionMismatch { found: version });
    }
    Ok(())
}

/// Whether a sqlx error is SQLite's "no such table" — the one failure that
/// legitimately means the file isn't a QMapShack database.
fn is_missing_table(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.message().contains("no such table"))
}

/// The database root folder (type 2) every folder chain hangs off.
pub async fn root_folder_id(conn: &mut SqliteConnection) -> Result<i64, TargetError> {
    sqlx::query_scalar("SELECT id FROM folders WHERE type = 2 ORDER BY id LIMIT 1")
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(TargetError::MissingRootFolder)
}

/// Folder-type enum values (`IDBItem.h`): groups for intermediate segments,
/// a project for the leaf that holds items — matching how QMapShack itself
/// nests group → project → items.
const FOLDER_GROUP: i64 = 3;
const FOLDER_PROJECT: i64 = 4;

/// Walk (and create where missing) the folder chain for `segments` under
/// `root`, returning the leaf folder's id. Existing folders are matched by
/// name regardless of type, so a pre-existing owner folder is reused rather
/// than duplicated; created folders are never deleted (ADR-0022 scoping).
pub async fn ensure_folder_path(
    conn: &mut SqliteConnection,
    root: i64,
    segments: &[String],
) -> Result<i64, sqlx::Error> {
    let mut parent = root;
    for (i, name) in segments.iter().enumerate() {
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT f.id FROM folders f \
             JOIN folder2folder ff ON ff.child = f.id \
             WHERE ff.parent = ? AND f.name = ? \
             ORDER BY f.id LIMIT 1",
        )
        .bind(parent)
        .bind(name)
        .fetch_optional(&mut *conn)
        .await?;

        parent = match existing {
            Some(id) => id,
            None => {
                let folder_type = if i + 1 == segments.len() {
                    FOLDER_PROJECT
                } else {
                    FOLDER_GROUP
                };
                let mut tx = conn.begin().await?;
                let id = sqlx::query("INSERT INTO folders (type, name) VALUES (?, ?)")
                    .bind(folder_type)
                    .bind(name)
                    .execute(&mut *tx)
                    .await?
                    .last_insert_rowid();
                sqlx::query("INSERT INTO folder2folder (parent, child) VALUES (?, ?)")
                    .bind(parent)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                id
            }
        };
    }
    Ok(parent)
}

/// Is there already an item with this `keyqms` (trash included)?
pub async fn item_exists(conn: &mut SqliteConnection, keyqms: &str) -> Result<bool, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items WHERE keyqms = ?")
        .bind(keyqms)
        .fetch_one(conn)
        .await?;
    Ok(count > 0)
}

/// Everything an `items` row needs (type is always 2 = track).
pub struct NewItem<'a> {
    pub keyqms: &'a str,
    pub name: &'a str,
    pub icon: &'a [u8],
    /// RFC-3339; QMapShack itself stores ISO timestamps here.
    pub date: &'a str,
    pub comment: &'a str,
    pub data: &'a [u8],
    pub hash: &'a str,
}

/// Insert one track item and link it under `folder_id`, in one target-side
/// transaction. The `searchindex_insert` and `folder2item_insert` triggers
/// handle the FTS row and the trash flag.
pub async fn insert_item(
    conn: &mut SqliteConnection,
    item: &NewItem<'_>,
    folder_id: i64,
) -> Result<(), sqlx::Error> {
    let mut tx = conn.begin().await?;
    let item_id = sqlx::query(
        "INSERT INTO items (type, keyqms, icon, name, date, comment, data, hash, last_user) \
         VALUES (2, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(item.keyqms)
    .bind(item.icon)
    .bind(item.name)
    .bind(item.date)
    .bind(item.comment)
    .bind(item.data)
    .bind(item.hash)
    .bind(super::blob::WHO)
    .execute(&mut *tx)
    .await?
    .last_insert_rowid();
    sqlx::query("INSERT INTO folder2item (parent, child) VALUES (?, ?)")
        .bind(folder_id)
        .bind(item_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
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

        let (root_type, root_name): (i64, String) =
            sqlx::query_as("SELECT type, name FROM folders")
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

    #[tokio::test]
    async fn item_exists_sees_inserted_items() {
        let (_dir, mut conn) = bootstrapped().await;
        let root = root_folder_id(&mut conn).await.unwrap();
        assert!(!item_exists(&mut conn, "trip-archive:trip:1").await.unwrap());
        insert_dummy_item(&mut conn, "trip-archive:trip:1", root).await;
        assert!(item_exists(&mut conn, "trip-archive:trip:1").await.unwrap());
        assert!(!item_exists(&mut conn, "trip-archive:trip:2").await.unwrap());
    }
}
