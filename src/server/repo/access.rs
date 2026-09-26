//! The access log's table (US-70, migration 0018): written in batches by
//! `access_log`'s writer, never on a request's own path.

use std::collections::HashMap;

use sqlx::{Row, SqlitePool};

use super::to_rfc3339;
use crate::config;
use crate::server::access_log::AccessRecord;
use crate::server::auth::Caller;
use crate::server::db;

/// Store `records` in one transaction, each user agent once. The IP address
/// a record carries for the stdout line is not stored.
pub async fn insert_access_records(
    pool: &SqlitePool,
    records: &[AccessRecord],
) -> Result<(), sqlx::Error> {
    let mut tx = db::begin_write(pool).await?;
    for record in records {
        let user_agent_id: Option<i64> = match &record.user_agent {
            Some(agent) => Some(
                sqlx::query_scalar(
                    "INSERT INTO user_agent (value) VALUES (?) \
                     ON CONFLICT (value) DO UPDATE SET value = excluded.value RETURNING id",
                )
                .bind(agent)
                .fetch_one(&mut *tx)
                .await?,
            ),
            None => None,
        };
        let (caller, share_id, share_label) = match &record.caller {
            Caller::Owner => ("owner", None, None),
            Caller::Anonymous => ("anonymous", None, None),
            Caller::UnknownLink => ("unknown_link", None, None),
            Caller::Share { id, label } => ("share", Some(*id), label.as_deref()),
        };
        sqlx::query(
            "INSERT INTO access_log \
             (at, method, path, status, duration_ms, caller, share_id, share_label, user_agent_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(to_rfc3339(record.at))
        .bind(&record.method)
        .bind(&record.path)
        .bind(i64::from(record.status))
        .bind(i64::try_from(record.duration_ms).unwrap_or(i64::MAX))
        .bind(caller)
        .bind(share_id)
        .bind(share_label)
        .bind(user_agent_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// How one share's link was used (US-70).
#[derive(Debug, Default, PartialEq)]
pub struct ShareUsage {
    pub opens: i64,
    pub last_opened_at: Option<String>,
    pub user_agents: Vec<String>,
}

/// Every share's openings, by share id: its screens loaded under its own
/// link (`/app/s/…`). The owner's preview is logged as the owner's, and the
/// data requests a screen then makes belong to the same opening, so neither
/// counts. The latest opening is the highest id, which follows arrival.
pub async fn share_usage(pool: &SqlitePool) -> Result<HashMap<i64, ShareUsage>, sqlx::Error> {
    let openings = format!("{}%", config::share::PAGE_PREFIX);
    let mut usage: HashMap<i64, ShareUsage> = HashMap::new();
    let counted = sqlx::query(
        r#"WITH opened AS (
               SELECT share_id, COUNT(*) AS opens, MAX(id) AS last_id FROM access_log
               WHERE caller = 'share' AND path LIKE ?
               GROUP BY share_id)
           SELECT o.share_id, o.opens, a.at FROM opened o JOIN access_log a ON a.id = o.last_id"#,
    )
    .bind(&openings)
    .fetch_all(pool)
    .await?;
    for row in counted {
        let entry = usage.entry(row.get("share_id")).or_default();
        entry.opens = row.get("opens");
        entry.last_opened_at = row.get("at");
    }
    let agents = sqlx::query(
        r#"SELECT DISTINCT l.share_id, u.value FROM access_log l
           JOIN user_agent u ON u.id = l.user_agent_id
           WHERE l.caller = 'share' AND l.path LIKE ?
           ORDER BY l.share_id, u.value"#,
    )
    .bind(&openings)
    .fetch_all(pool)
    .await?;
    for row in agents {
        usage
            .entry(row.get("share_id"))
            .or_default()
            .user_agents
            .push(row.get("value"));
    }
    Ok(usage)
}
