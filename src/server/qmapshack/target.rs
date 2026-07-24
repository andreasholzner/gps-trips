//! Access to the *target* QMapShack database (US-36/US-37). This is a foreign,
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

/// A previously-exported item's state, as far as US-37 change detection
/// cares (ADR-0022 as amended): the cheap columns plus folder placement.
#[derive(Debug)]
pub struct ItemState {
    pub id: i64,
    pub name: String,
    pub comment: Option<String>,
    /// Parent folder ids, ascending; empty means trashed (no links left).
    pub folder_ids: Vec<i64>,
}

/// Look up the item with this `keyqms` (trash included), or `None`.
pub async fn get_item_state(
    conn: &mut SqliteConnection,
    keyqms: &str,
) -> Result<Option<ItemState>, sqlx::Error> {
    let Some(row) = sqlx::query("SELECT id, name, comment FROM items WHERE keyqms = ?")
        .bind(keyqms)
        .fetch_optional(&mut *conn)
        .await?
    else {
        return Ok(None);
    };
    let id: i64 = row.get("id");
    let folder_ids =
        sqlx::query_scalar("SELECT parent FROM folder2item WHERE child = ? ORDER BY parent")
            .bind(id)
            .fetch_all(&mut *conn)
            .await?;
    Ok(Some(ItemState {
        id,
        name: row.get("name"),
        comment: row.get("comment"),
        folder_ids,
    }))
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

/// Rewrite an existing item in place (US-37): same full-blob-rewrite
/// semantics as an insert (ADR-0022), touching every column an insert sets.
/// The `items_update_last_change` and `searchindex_update` triggers keep
/// `last_change` and the FTS row in step.
pub async fn update_item(
    conn: &mut SqliteConnection,
    id: i64,
    item: &NewItem<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE items SET icon = ?, name = ?, date = ?, comment = ?, \
         data = ?, hash = ?, last_user = ? WHERE id = ?",
    )
    .bind(item.icon)
    .bind(item.name)
    .bind(item.date)
    .bind(item.comment)
    .bind(item.data)
    .bind(item.hash)
    .bind(super::blob::WHO)
    .bind(id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Reconcile an item's placement to exactly `folder_id` (US-37): the
/// exporter's mapping is authoritative, so extra links the owner added go
/// away and a trashed item comes back. The desired link is inserted *first*
/// (clearing `trash` via `folder2item_insert`), then every other link is
/// dropped — insert-first so `folder2item_delete` never sees the item fully
/// unlinked and stamps a spurious trash timestamp.
pub async fn set_item_folder(
    conn: &mut SqliteConnection,
    id: i64,
    folder_id: i64,
) -> Result<(), sqlx::Error> {
    let mut tx = conn.begin().await?;
    let already: Option<i64> =
        sqlx::query_scalar("SELECT MIN(id) FROM folder2item WHERE child = ? AND parent = ?")
            .bind(id)
            .bind(folder_id)
            .fetch_one(&mut *tx)
            .await?;
    let keep = match already {
        Some(link_id) => link_id,
        None => sqlx::query("INSERT INTO folder2item (parent, child) VALUES (?, ?)")
            .bind(folder_id)
            .bind(id)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid(),
    };
    // Deleting by link id (not by parent) also collapses duplicate links to
    // the desired folder, which would otherwise never reconcile.
    sqlx::query("DELETE FROM folder2item WHERE child = ? AND id != ?")
        .bind(id)
        .bind(keep)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Drop every folder link of an item, letting QMapShack's own
/// `folder2item_delete` trigger move it to the trash — the US-37 removal
/// path for trips deleted from the archive (US-9).
pub async fn unlink_item(conn: &mut SqliteConnection, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM folder2item WHERE child = ?")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

/// One row of the US-37 removal scan.
#[derive(Debug)]
pub struct ExporterItem {
    pub id: i64,
    pub keyqms: String,
    /// Still linked under at least one folder (false = already in trash).
    pub linked: bool,
}

/// Every item whose `keyqms` carries the exporter's namespace prefix —
/// the only rows reconciliation may ever touch (ADR-0022 scoping). Fetches
/// all rows and filters in Rust: an exact prefix match, no SQL `LIKE`
/// pattern semantics to worry about.
pub async fn list_exporter_items(
    conn: &mut SqliteConnection,
    prefix: &str,
) -> Result<Vec<ExporterItem>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT i.id, i.keyqms, \
         EXISTS(SELECT 1 FROM folder2item f2i WHERE f2i.child = i.id) AS linked \
         FROM items i",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let keyqms: String = row.get("keyqms");
            keyqms.starts_with(prefix).then(|| ExporterItem {
                id: row.get("id"),
                linked: row.get("linked"),
                keyqms,
            })
        })
        .collect())
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────
// Split into target/tests.rs to keep this file under the 650-line hard cap.

#[cfg(test)]
mod tests;
