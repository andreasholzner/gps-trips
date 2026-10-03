//! Shares (US-53): a token, the trips it reaches, and when it stops; and
//! the owner's list of them, from which one is stopped (US-69).
//!
//! A share names trips, or tags whose recorded trips it reaches (US-82).
//! Which trips that is comes from the `share_reach` view alone, so every
//! check here agrees on it. Every read is scoped by the share's id, which
//! only the gate hands out, and only for a token that resolved — so a
//! handler that reads through these functions cannot reach a trip its share
//! does not name.

use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::to_rfc3339;
use super::trip::local_start_date;
use crate::models::{ActiveShare, SharedTripSummary};
use crate::server::db;

/// What a share names.
pub enum ShareTarget<'a> {
    Trips(&'a [i64]),
    /// Tag ids, in the order the owner chose them (US-82).
    Tags(&'a [i64]),
}

/// A share as the owner asks for it; the token is minted by the caller.
pub struct NewShare<'a> {
    pub token: &'a str,
    pub label: Option<&'a str>,
    pub target: ShareTarget<'a>,
    pub created_at: OffsetDateTime,
    pub expires_at: Option<OffsetDateTime>,
}

/// Store a share and what it names, in one transaction. The caller has
/// checked the trips or tags exist.
pub async fn insert_share(pool: &SqlitePool, share: &NewShare<'_>) -> Result<i64, sqlx::Error> {
    let mut tx = db::begin_write(pool).await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO share (token, label, created_at, expires_at) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(share.token)
    .bind(share.label)
    .bind(to_rfc3339(share.created_at))
    .bind(share.expires_at.map(to_rfc3339))
    .fetch_one(&mut *tx)
    .await?;
    match share.target {
        ShareTarget::Trips(trip_ids) => {
            for &trip_id in trip_ids {
                sqlx::query("INSERT OR IGNORE INTO share_trip (share_id, trip_id) VALUES (?, ?)")
                    .bind(id)
                    .bind(trip_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        ShareTarget::Tags(tag_ids) => {
            for (position, &tag_id) in (0_i64..).zip(tag_ids) {
                sqlx::query(
                    "INSERT OR IGNORE INTO share_tag (share_id, tag_id, position) VALUES (?, ?, ?)",
                )
                .bind(id)
                .bind(tag_id)
                .bind(position)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(id)
}

/// A share a token opened: its id, which scopes every read the recipient's
/// routes make, and its label, which names it in the access log (US-70).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedShare {
    pub id: i64,
    pub label: Option<String>,
}

/// Whether share `s` still names something: a trip, or a tag (US-82) —
/// which keeps it alive while its tags hold no trips, since tagging can
/// fill it again.
const NAMES_SOMETHING: &str = "(EXISTS (SELECT 1 FROM share_trip st WHERE st.share_id = s.id) \
     OR EXISTS (SELECT 1 FROM share_tag sg WHERE sg.share_id = s.id))";

/// The share `token` opens at `now`: one that exists, has not expired and
/// still names something. Anything else is `None`, and deliberately the same
/// `None` — a link must not tell a stranger whether it ever worked.
pub async fn resolve_share(
    pool: &SqlitePool,
    token: &str,
    now: OffsetDateTime,
) -> Result<Option<ResolvedShare>, sqlx::Error> {
    let row: Option<(i64, Option<String>, Option<String>)> = sqlx::query_as(&format!(
        "SELECT s.id, s.label, s.expires_at FROM share s WHERE s.token = ? AND {NAMES_SOMETHING}"
    ))
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(id, label, expires_at)| {
        still_open(expires_at.as_deref(), now).then_some(ResolvedShare { id, label })
    }))
}

/// Whether a share expiring at `expires_at` still opens at `now`. Compared
/// as instants, not as strings: RFC-3339 text only sorts like time when
/// every value has the same precision. An unreadable expiry opens nothing.
fn still_open(expires_at: Option<&str>, now: OffsetDateTime) -> bool {
    expires_at.is_none_or(|at| OffsetDateTime::parse(at, &Rfc3339).is_ok_and(|at| at > now))
}

/// Every share that opens something at `now` — the same test
/// [`resolve_share`] applies — newest first, each with its trips' names in
/// the order the recipient sees them, or its tags in the order chosen
/// (US-82), and how its link was used (US-70).
pub async fn list_active_shares(
    pool: &SqlitePool,
    now: OffsetDateTime,
) -> Result<Vec<ActiveShare>, sqlx::Error> {
    // Ids only grow, so the highest is the newest without comparing
    // timestamps as text.
    let mut shares: Vec<ActiveShare> = sqlx::query(&format!(
        "SELECT s.id, s.token, s.label, s.created_at, s.expires_at FROM share s \
         WHERE {NAMES_SOMETHING} ORDER BY s.id DESC"
    ))
    .map(|row: SqliteRow| ActiveShare {
        id: row.get("id"),
        token: row.get("token"),
        label: row.get("label"),
        trip_names: Vec::new(),
        tags: Vec::new(),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
        opens: 0,
        last_opened_at: None,
        user_agents: Vec::new(),
    })
    .fetch_all(pool)
    .await?;
    shares.retain(|share| still_open(share.expires_at.as_deref(), now));

    let trip_names: Vec<(i64, String)> = sqlx::query_as(
        r#"SELECT st.share_id, t.name FROM share_trip st JOIN trip t ON t.id = st.trip_id
           ORDER BY t.start_time IS NULL, t.start_time, t.id"#,
    )
    .fetch_all(pool)
    .await?;
    let tag_names: Vec<(i64, String)> = sqlx::query_as(
        r#"SELECT sg.share_id, tag.name FROM share_tag sg JOIN tag ON tag.id = sg.tag_id
           ORDER BY sg.position"#,
    )
    .fetch_all(pool)
    .await?;
    for share in &mut shares {
        share.trip_names = names_of(&trip_names, share.id);
        share.tags = names_of(&tag_names, share.id);
    }

    // How each link was used (US-70), from the access log.
    let mut usage = super::access::share_usage(pool).await?;
    for share in &mut shares {
        if let Some(used) = usage.remove(&share.id) {
            share.opens = used.opens;
            share.last_opened_at = used.last_opened_at;
            share.user_agents = used.user_agents;
        }
    }
    Ok(shares)
}

/// The names in `rows` that belong to share `id`, in the rows' order.
fn names_of(rows: &[(i64, String)], id: i64) -> Vec<String> {
    rows.iter()
        .filter(|(share_id, _)| *share_id == id)
        .map(|(_, name)| name.clone())
        .collect()
}

/// Stop share `id`: its row goes, and with it every `share_trip` and
/// `share_tag` row, so its token resolves to nothing from the next request
/// on. Only a share that still opens something at `now` is stopped; `false`
/// for any other, so stopping one twice is not a success.
pub async fn stop_share(
    pool: &SqlitePool,
    id: i64,
    now: OffsetDateTime,
) -> Result<bool, sqlx::Error> {
    let found: Option<Option<String>> = sqlx::query_scalar(&format!(
        "SELECT s.expires_at FROM share s WHERE s.id = ? AND {NAMES_SOMETHING}"
    ))
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
        sqlx::query_scalar("SELECT 1 FROM share_reach WHERE share_id = ? AND trip_id = ?")
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
           JOIN share_reach st ON st.trip_id = p.trip_id
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
        r#"SELECT t.id, t.name, t.activity_type, t.start_time, t.tz_name, t.distance_m,
                  t.ascent_m, t.duration_secs
           FROM trip t JOIN share_reach st ON st.trip_id = t.id
           WHERE st.share_id = ?
           ORDER BY t.start_time IS NULL, t.start_time, t.id"#,
    )
    .bind(share_id)
    .map(row_to_shared_summary)
    .fetch_all(pool)
    .await
}

/// The tags `share_id` names, in the order the owner chose them (US-82);
/// empty for a share of trips.
pub async fn shared_tag_names(
    pool: &SqlitePool,
    share_id: i64,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT tag.name FROM share_tag sg JOIN tag ON tag.id = sg.tag_id
           WHERE sg.share_id = ? ORDER BY sg.position"#,
    )
    .bind(share_id)
    .fetch_all(pool)
    .await
}

fn row_to_shared_summary(row: SqliteRow) -> SharedTripSummary {
    let start_time: Option<String> = row.get("start_time");
    let tz_name: Option<String> = row.get("tz_name");
    SharedTripSummary {
        id: row.get("id"),
        name: row.get("name"),
        activity_type: row.get("activity_type"),
        start_date: local_start_date(start_time.as_deref(), tz_name.as_deref()),
        start_time,
        distance_m: row.get("distance_m"),
        ascent_m: row.get("ascent_m"),
        duration_secs: row.get("duration_secs"),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
