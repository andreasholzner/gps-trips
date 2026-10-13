//! Tag CRUD and the links putting tags on trips (US-33), collections and
//! their tracks (US-93), and the Tags screen's
//! listing, creating and deleting (US-83) and renaming (US-85) of tags. Kept
//! separate from `trip`/`photo`, mirroring how each domain gets its own repo
//! submodule.

use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqlitePool};
use time::OffsetDateTime;

use crate::models::{Tag, TagOverview, TagShare};
use crate::server::db;

/// Get the id of the tag named `name` (already normalized by the caller),
/// creating it if it doesn't exist yet (US-33: "using a new tag creates the
/// tag on-demand"). A single upsert round-trip rather than a
/// select-then-insert — that shape would race a concurrent request creating
/// the same tag between the select and the insert.
pub async fn get_or_create_tag(pool: &SqlitePool, name: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        r#"INSERT INTO tag (name) VALUES (?)
           ON CONFLICT(name) DO UPDATE SET name = excluded.name
           RETURNING id"#,
    )
    .bind(name)
    .fetch_one(pool)
    .await
}

/// What a tag can be put on: a trip (US-33), or a collection or one of its
/// tracks (US-93). Each has links of its own (ADR-0028), so nothing that
/// reads a trip's tags sees a collection's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tagged {
    Trip(i64),
    Collection(i64),
    CollectionTrack(i64),
}

impl Tagged {
    /// The link table, the column naming what is tagged, and its id.
    fn link(self) -> (&'static str, &'static str, i64) {
        match self {
            Tagged::Trip(id) => ("trip_tag", "trip_id", id),
            Tagged::Collection(id) => ("collection_tag", "collection_id", id),
            Tagged::CollectionTrack(id) => ("collection_track_tag", "collection_track_id", id),
        }
    }
}

/// Whether what is to be tagged exists, on the pool or a transaction.
pub async fn tagged_exists<'e, E>(executor: E, tagged: Tagged) -> Result<bool, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let (table, id) = match tagged {
        Tagged::Trip(id) => ("trip", id),
        Tagged::Collection(id) => ("collection", id),
        Tagged::CollectionTrack(id) => ("collection_track", id),
    };
    sqlx::query_scalar(&format!(
        "SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?)"
    ))
    .bind(id)
    .fetch_one(executor)
    .await
}

/// Tag what `tagged` names with the tag `name` (already normalized by the
/// caller), creating the tag on demand (US-33, US-93). One transaction
/// checks that it exists, creates the tag and links it, so something
/// deleted meanwhile is `None` — and leaves no new tag behind — rather than
/// a foreign-key failure.
pub async fn add_tag_named(
    pool: &SqlitePool,
    tagged: Tagged,
    name: &str,
) -> Result<Option<Tag>, sqlx::Error> {
    let mut tx = db::begin_write(pool).await?;
    if !tagged_exists(&mut *tx, tagged).await? {
        return Ok(None);
    }
    let tag_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO tag (name) VALUES (?)
           ON CONFLICT(name) DO UPDATE SET name = excluded.name
           RETURNING id"#,
    )
    .bind(name)
    .fetch_one(&mut *tx)
    .await?;
    let (table, column, id) = tagged.link();
    sqlx::query(&format!(
        "INSERT OR IGNORE INTO {table} ({column}, tag_id) VALUES (?, ?)"
    ))
    .bind(id)
    .bind(tag_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(Tag {
        id: tag_id,
        name: name.to_string(),
    }))
}

/// Link a tag to what it is put on. Idempotent: tagging with a tag already
/// carried is a no-op, not an error.
pub async fn add_tag(pool: &SqlitePool, tagged: Tagged, tag_id: i64) -> Result<(), sqlx::Error> {
    let (table, column, id) = tagged.link();
    sqlx::query(&format!(
        "INSERT OR IGNORE INTO {table} ({column}, tag_id) VALUES (?, ?)"
    ))
    .bind(id)
    .bind(tag_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Unlink a tag. The `tag` row itself is left in place (kept for
/// reuse/autocomplete, US-33) even if this was its last link. Returns `true`
/// if a link existed and was removed.
pub async fn remove_tag(
    pool: &SqlitePool,
    tagged: Tagged,
    tag_id: i64,
) -> Result<bool, sqlx::Error> {
    let (table, column, id) = tagged.link();
    let result = sqlx::query(&format!(
        "DELETE FROM {table} WHERE {column} = ? AND tag_id = ?"
    ))
    .bind(id)
    .bind(tag_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// The tags something carries, alphabetical.
pub async fn list_tags_of(pool: &SqlitePool, tagged: Tagged) -> Result<Vec<Tag>, sqlx::Error> {
    let (table, column, id) = tagged.link();
    sqlx::query(&format!(
        "SELECT tag.id, tag.name FROM tag \
         JOIN {table} link ON link.tag_id = tag.id \
         WHERE link.{column} = ? \
         ORDER BY tag.name"
    ))
    .bind(id)
    .map(row_to_tag)
    .fetch_all(pool)
    .await
}

/// Every tag that exists, alphabetical — feeds the trip detail page's
/// autocomplete suggestions (US-33).
pub async fn list_all_tags(pool: &SqlitePool) -> Result<Vec<Tag>, sqlx::Error> {
    sqlx::query("SELECT id, name FROM tag ORDER BY name")
        .map(row_to_tag)
        .fetch_all(pool)
        .await
}

/// Every tag, alphabetical, with how many trips — and how many recorded
/// trips — carry it, and the summary shares (US-82) that still open at `now`
/// and name it (US-83); and how many collections carry it, and how many
/// collection tracks whose collection does not (US-93). Counted one table at
/// a time: joining them all would count each trip once per collection.
pub async fn list_tag_overview(
    pool: &SqlitePool,
    now: OffsetDateTime,
) -> Result<Vec<TagOverview>, sqlx::Error> {
    let mut tags: Vec<TagOverview> = sqlx::query(
        r#"SELECT tag.id, tag.name,
                  (SELECT COUNT(*) FROM trip_tag tt WHERE tt.tag_id = tag.id) AS trip_count,
                  (SELECT COUNT(*) FROM trip_tag tt JOIN trip t ON t.id = tt.trip_id
                   WHERE tt.tag_id = tag.id AND t.trip_kind = 'recorded') AS recorded_trip_count,
                  (SELECT COUNT(*) FROM collection_tag ct
                   WHERE ct.tag_id = tag.id) AS collection_count,
                  (SELECT COUNT(*) FROM collection_track_tag ctt
                   JOIN collection_track track ON track.id = ctt.collection_track_id
                   WHERE ctt.tag_id = tag.id
                     AND NOT EXISTS (SELECT 1 FROM collection_tag ct
                                     WHERE ct.tag_id = tag.id
                                       AND ct.collection_id = track.collection_id))
                      AS collection_track_count
           FROM tag
           ORDER BY tag.name"#,
    )
    .map(|row: SqliteRow| TagOverview {
        id: row.get("id"),
        name: row.get("name"),
        trip_count: row.get("trip_count"),
        recorded_trip_count: row.get("recorded_trip_count"),
        collection_count: row.get("collection_count"),
        collection_track_count: row.get("collection_track_count"),
        shares: Vec::new(),
    })
    .fetch_all(pool)
    .await?;

    // The Shares screen's own list, so the two agree on which are active.
    for share in super::list_active_shares(pool, now).await? {
        for tag in tags.iter_mut().filter(|tag| share.tags.contains(&tag.name)) {
            tag.shares.push(TagShare {
                id: share.id,
                label: share.label.clone(),
                tags: share.tags.clone(),
            });
        }
    }
    Ok(tags)
}

/// Create the tag `name` (already normalized), carrying no trips (US-83).
/// `None` if a tag of that name exists already, which is left as it is.
pub async fn create_tag(pool: &SqlitePool, name: &str) -> Result<Option<Tag>, sqlx::Error> {
    let id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO tag (name) VALUES (?) ON CONFLICT(name) DO NOTHING RETURNING id",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(id.map(|id| Tag {
        id,
        name: name.to_string(),
    }))
}

/// What renaming a tag came to (US-85).
#[derive(Debug, PartialEq, Eq)]
pub enum Rename {
    Renamed(Tag),
    /// There is no such tag.
    Missing,
    /// Another tag already has the name.
    Taken,
}

/// Rename tag `id` to `name` (already normalized by the caller) (US-85).
/// Trips and shares name the tag by its id, so they all carry the new name
/// at once. Renaming a tag to its own name is no change and no conflict.
pub async fn rename_tag(pool: &SqlitePool, id: i64, name: &str) -> Result<Rename, sqlx::Error> {
    let mut tx = db::begin_write(pool).await?;
    let renamed: Option<i64> =
        sqlx::query_scalar("UPDATE OR IGNORE tag SET name = ? WHERE id = ? RETURNING id")
            .bind(name)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let outcome = if renamed.is_some() {
        Rename::Renamed(Tag {
            id,
            name: name.to_string(),
        })
    } else {
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM tag WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        match exists {
            Some(_) => Rename::Taken,
            None => Rename::Missing,
        }
    };
    tx.commit().await?;
    Ok(outcome)
}

/// Delete tag `id` (US-83), in one transaction. A share naming no other tag
/// is stopped — its row goes, as `stop_share` would take it — whether or not
/// it has expired; the tag's own row then goes, and the cascade takes it off
/// every trip, collection and collection track (US-93) and out of every
/// other share, which keeps its other tags in their order. Nothing it was on
/// is otherwise touched. `false` if there is no such tag.
pub async fn delete_tag(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let mut tx = db::begin_write(pool).await?;
    sqlx::query(
        r#"DELETE FROM share WHERE id IN (
               SELECT share_id FROM share_tag GROUP BY share_id
               HAVING COUNT(*) = 1 AND MAX(tag_id) = ?)"#,
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let deleted = sqlx::query("DELETE FROM tag WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(deleted.rows_affected() > 0)
}

fn row_to_tag(row: SqliteRow) -> Tag {
    Tag {
        id: row.get("id"),
        name: row.get("name"),
    }
}

/// The ids of the tags named in `names` (already normalized), in the same
/// order — `None` if any of them does not exist (US-82).
pub async fn find_tag_ids(
    pool: &SqlitePool,
    names: &[String],
) -> Result<Option<Vec<i64>>, sqlx::Error> {
    let mut ids = Vec::with_capacity(names.len());
    for name in names {
        let id: Option<i64> = sqlx::query_scalar("SELECT id FROM tag WHERE name = ?")
            .bind(name)
            .fetch_optional(pool)
            .await?;
        let Some(id) = id else {
            return Ok(None);
        };
        ids.push(id);
    }
    Ok(Some(ids))
}

/// Whether every id in `trip_ids` is an existing trip's id: a trip share's
/// precondition (US-53).
pub async fn trips_exist(pool: &SqlitePool, trip_ids: &[i64]) -> Result<bool, sqlx::Error> {
    for &trip_id in trip_ids {
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM trip WHERE id = ?")
            .bind(trip_id)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Apply every tag named in `tag_names` (already normalized by the caller) to
/// everything in `tagged` — trips selected on the list page (US-34), or
/// collections (US-100) — in one transaction: each tag is created on demand —
/// the same upsert `get_or_create_tag` uses, inlined here because it needs to
/// run on this call's transaction, not a fresh pool connection — and linked
/// to each through its own link table via `INSERT OR IGNORE`, making
/// re-application idempotent just like `add_tag`. A name repeated in
/// `tag_names` is only applied once — otherwise the DB writes stay
/// idempotent but the returned `Tag` list would contain the same tag twice.
/// Returns the applied tags — or `None` if anything in `tagged` does not
/// exist, checked on the same transaction as `add_tag_named` does, so
/// something deleted meanwhile tags nothing and leaves no new tag behind
/// rather than failing a foreign key (US-34's and US-100's all or nothing).
pub async fn bulk_add_tags(
    pool: &SqlitePool,
    tagged: &[Tagged],
    tag_names: &[String],
) -> Result<Option<Vec<Tag>>, sqlx::Error> {
    let mut tx = db::begin_write(pool).await?;
    for &each in tagged {
        if !tagged_exists(&mut *tx, each).await? {
            return Ok(None);
        }
    }
    let mut tags = Vec::with_capacity(tag_names.len());
    let mut seen = std::collections::HashSet::with_capacity(tag_names.len());

    for name in tag_names {
        if !seen.insert(name.as_str()) {
            continue;
        }

        let tag_id: i64 = sqlx::query_scalar(
            r#"INSERT INTO tag (name) VALUES (?)
               ON CONFLICT(name) DO UPDATE SET name = excluded.name
               RETURNING id"#,
        )
        .bind(name)
        .fetch_one(&mut *tx)
        .await?;

        for &each in tagged {
            let (table, column, id) = each.link();
            sqlx::query(&format!(
                "INSERT OR IGNORE INTO {table} ({column}, tag_id) VALUES (?, ?)"
            ))
            .bind(id)
            .bind(tag_id)
            .execute(&mut *tx)
            .await?;
        }

        tags.push(Tag {
            id: tag_id,
            name: name.clone(),
        });
    }

    tx.commit().await?;
    Ok(Some(tags))
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
