//! Shares (US-53): a token, the trips it reaches, and when it stops; and
//! the owner's list of them, from which one is stopped (US-69).
//!
//! Every read here is scoped by the share's id, which only the gate hands
//! out, and only for a token that resolved — so a handler that reads through
//! these functions cannot reach a trip its share does not name.

use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::to_rfc3339;
use crate::models::{ActiveShare, SharedTripSummary};

/// A share as the owner asks for it; the token is minted by the caller.
pub struct NewShare<'a> {
    pub token: &'a str,
    pub label: Option<&'a str>,
    pub trip_ids: &'a [i64],
    pub created_at: OffsetDateTime,
    pub expires_at: Option<OffsetDateTime>,
}

/// Store a share and the trips it reaches, in one transaction. The caller
/// has checked the trips exist.
pub async fn insert_share(pool: &SqlitePool, share: &NewShare<'_>) -> Result<i64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO share (token, label, created_at, expires_at) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(share.token)
    .bind(share.label)
    .bind(to_rfc3339(share.created_at))
    .bind(share.expires_at.map(to_rfc3339))
    .fetch_one(&mut *tx)
    .await?;
    for &trip_id in share.trip_ids {
        sqlx::query("INSERT OR IGNORE INTO share_trip (share_id, trip_id) VALUES (?, ?)")
            .bind(id)
            .bind(trip_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(id)
}

/// The share `token` opens at `now`: one that exists, has not expired and
/// still reaches a trip. Anything else is `None`, and deliberately the same
/// `None` — a link must not tell a stranger whether it ever worked.
pub async fn resolve_share(
    pool: &SqlitePool,
    token: &str,
    now: OffsetDateTime,
) -> Result<Option<i64>, sqlx::Error> {
    let row: Option<(i64, Option<String>)> = sqlx::query_as(
        r#"SELECT s.id, s.expires_at FROM share s
           WHERE s.token = ?
             AND EXISTS (SELECT 1 FROM share_trip st WHERE st.share_id = s.id)"#,
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(id, expires_at)| still_open(expires_at.as_deref(), now).then_some(id)))
}

/// Whether a share expiring at `expires_at` still opens at `now`. Compared
/// as instants, not as strings: RFC-3339 text only sorts like time when
/// every value has the same precision. An unreadable expiry opens nothing.
fn still_open(expires_at: Option<&str>, now: OffsetDateTime) -> bool {
    expires_at.is_none_or(|at| OffsetDateTime::parse(at, &Rfc3339).is_ok_and(|at| at > now))
}

/// Every share that opens something at `now` — the same test
/// [`resolve_share`] applies — newest first, each with its trips' names in
/// the order the recipient sees them.
pub async fn list_active_shares(
    pool: &SqlitePool,
    now: OffsetDateTime,
) -> Result<Vec<ActiveShare>, sqlx::Error> {
    // The inner join leaves out a share with no trips. Ids only grow, so the
    // highest is the newest without comparing timestamps as text.
    let rows = sqlx::query(
        r#"SELECT s.id, s.token, s.label, s.created_at, s.expires_at, t.name
           FROM share s
           JOIN share_trip st ON st.share_id = s.id
           JOIN trip t ON t.id = st.trip_id
           ORDER BY s.id DESC, t.start_time IS NULL, t.start_time, t.id"#,
    )
    .fetch_all(pool)
    .await?;

    let mut shares: Vec<ActiveShare> = Vec::new();
    for row in rows {
        let id: i64 = row.get("id");
        let name: String = row.get("name");
        match shares.last_mut() {
            Some(share) if share.id == id => share.trip_names.push(name),
            _ => shares.push(ActiveShare {
                id,
                token: row.get("token"),
                label: row.get("label"),
                trip_names: vec![name],
                created_at: row.get("created_at"),
                expires_at: row.get("expires_at"),
            }),
        }
    }
    shares.retain(|share| still_open(share.expires_at.as_deref(), now));
    Ok(shares)
}

/// Stop share `id`: its row goes, and with it every `share_trip` row, so
/// its token resolves to nothing from the next request on. Only a share
/// that still opens something at `now` is stopped; `false` for any other,
/// so stopping one twice is not a success.
pub async fn stop_share(
    pool: &SqlitePool,
    id: i64,
    now: OffsetDateTime,
) -> Result<bool, sqlx::Error> {
    let found: Option<Option<String>> = sqlx::query_scalar(
        r#"SELECT s.expires_at FROM share s
           WHERE s.id = ?
             AND EXISTS (SELECT 1 FROM share_trip st WHERE st.share_id = s.id)"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    if !found.is_some_and(|expires_at| still_open(expires_at.as_deref(), now)) {
        return Ok(false);
    }
    let deleted = sqlx::query("DELETE FROM share WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(deleted.rows_affected() > 0)
}

/// Whether `share_id` reaches `trip_id`.
pub async fn share_covers_trip(
    pool: &SqlitePool,
    share_id: i64,
    trip_id: i64,
) -> Result<bool, sqlx::Error> {
    let found: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM share_trip WHERE share_id = ? AND trip_id = ?")
            .bind(share_id)
            .bind(trip_id)
            .fetch_optional(pool)
            .await?;
    Ok(found.is_some())
}

/// Whether `key` is a photo — full size or thumbnail — of a trip `share_id`
/// reaches. Asked of the database rather than read off the key's shape, so a
/// key crafted to climb out of its trip's directory is simply not one.
pub async fn share_covers_blob(
    pool: &SqlitePool,
    share_id: i64,
    key: &str,
) -> Result<bool, sqlx::Error> {
    let found: Option<i64> = sqlx::query_scalar(
        r#"SELECT 1 FROM photo p
           JOIN share_trip st ON st.trip_id = p.trip_id
           WHERE st.share_id = ? AND (p.blob_key = ? OR p.thumbnail_key = ?)
           LIMIT 1"#,
    )
    .bind(share_id)
    .bind(key)
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

/// The share's label.
pub async fn share_label(pool: &SqlitePool, share_id: i64) -> Result<Option<String>, sqlx::Error> {
    let label: Option<Option<String>> = sqlx::query_scalar("SELECT label FROM share WHERE id = ?")
        .bind(share_id)
        .fetch_optional(pool)
        .await?;
    Ok(label.flatten())
}

/// The trips `share_id` reaches, oldest first — the order a series of trips
/// is told in. A trip without times sorts last.
pub async fn list_shared_trips(
    pool: &SqlitePool,
    share_id: i64,
) -> Result<Vec<SharedTripSummary>, sqlx::Error> {
    sqlx::query(
        r#"SELECT t.id, t.name, t.activity_type, t.start_time, t.distance_m, t.ascent_m,
                  t.duration_secs
           FROM trip t JOIN share_trip st ON st.trip_id = t.id
           WHERE st.share_id = ?
           ORDER BY t.start_time IS NULL, t.start_time, t.id"#,
    )
    .bind(share_id)
    .map(row_to_shared_summary)
    .fetch_all(pool)
    .await
}

fn row_to_shared_summary(row: SqliteRow) -> SharedTripSummary {
    SharedTripSummary {
        id: row.get("id"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
        start_time: row.get("start_time"),
        distance_m: row.get("distance_m"),
        ascent_m: row.get("ascent_m"),
        duration_secs: row.get("duration_secs"),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
