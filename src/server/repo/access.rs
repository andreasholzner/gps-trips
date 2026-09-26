//! The access log's table (US-70, migration 0018): written in batches by
//! `access_log`'s writer, never on a request's own path.

use sqlx::SqlitePool;

use super::to_rfc3339;
use crate::server::access_log::AccessRecord;
use crate::server::auth::Caller;

/// Store `records` in one transaction, each user agent once. The IP address
/// a record carries for the stdout line is not stored.
pub async fn insert_access_records(
    pool: &SqlitePool,
    records: &[AccessRecord],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
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
