//! Tag CRUD and trip/tag associations (US-33). Kept separate from `trip`/
//! `photo`, mirroring how each domain gets its own repo submodule.

use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use crate::models::Tag;

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

/// Link a tag to a trip. Idempotent: tagging a trip with a tag it already
/// carries is a no-op, not an error.
pub async fn add_trip_tag(pool: &SqlitePool, trip_id: i64, tag_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT OR IGNORE INTO trip_tag (trip_id, tag_id) VALUES (?, ?)")
        .bind(trip_id)
        .bind(tag_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Unlink a tag from a trip. The `tag` row itself is left in place (kept for
/// reuse/autocomplete, US-33) even if this was its last trip. Returns `true`
/// if a link existed and was removed.
pub async fn remove_trip_tag(
    pool: &SqlitePool,
    trip_id: i64,
    tag_id: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM trip_tag WHERE trip_id = ? AND tag_id = ?")
        .bind(trip_id)
        .bind(tag_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// A trip's current tags, alphabetical.
pub async fn list_trip_tags(pool: &SqlitePool, trip_id: i64) -> Result<Vec<Tag>, sqlx::Error> {
    sqlx::query(
        r#"SELECT tag.id, tag.name FROM tag
           JOIN trip_tag ON trip_tag.tag_id = tag.id
           WHERE trip_tag.trip_id = ?
           ORDER BY tag.name"#,
    )
    .bind(trip_id)
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

fn row_to_tag(row: SqliteRow) -> Tag {
    Tag {
        id: row.get("id"),
        name: row.get("name"),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ActivityType, TripKind};
    use crate::server::db::testing::TestDb;
    use crate::server::geojson::build_track_geojson;
    use crate::server::gpx::{compute_stats, parse_gpx};
    use crate::server::repo::{insert_trip, NewTrip};

    const SAMPLE_GPX: &[u8] = include_bytes!("../../../tests/fixtures/sample.gpx");

    async fn insert_sample_trip(pool: &SqlitePool) -> i64 {
        let track = parse_gpx(SAMPLE_GPX).unwrap();
        let stats = compute_stats(&track.points);
        let geojson = build_track_geojson(&track.points);
        insert_trip(
            pool,
            &NewTrip {
                name: "Oslo Hills Walk",
                activity_type: ActivityType::Hiking,
                tz_name: "Europe/Oslo",
                stats: &stats,
                geojson: &geojson,
                gpx: SAMPLE_GPX,
                trip_kind: TripKind::Recorded,
            },
        )
        .await
        .expect("insert_trip")
    }

    #[tokio::test]
    async fn get_or_create_tag_creates_a_new_tag() {
        let db = TestDb::new().await;
        let id = get_or_create_tag(&db.pool, "hiking").await.unwrap();
        assert!(id > 0);

        let all = list_all_tags(&db.pool).await.unwrap();
        assert_eq!(
            all,
            vec![Tag {
                id,
                name: "hiking".to_string()
            }]
        );
    }

    #[tokio::test]
    async fn get_or_create_tag_returns_the_same_id_for_an_existing_name() {
        let db = TestDb::new().await;
        let first = get_or_create_tag(&db.pool, "hiking").await.unwrap();
        let second = get_or_create_tag(&db.pool, "hiking").await.unwrap();
        assert_eq!(first, second);
        assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn add_trip_tag_links_the_tag_to_the_trip() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;
        let tag_id = get_or_create_tag(&db.pool, "hiking").await.unwrap();

        add_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();

        let tags = list_trip_tags(&db.pool, trip_id).await.unwrap();
        assert_eq!(
            tags,
            vec![Tag {
                id: tag_id,
                name: "hiking".to_string()
            }]
        );
    }

    #[tokio::test]
    async fn add_trip_tag_is_idempotent() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;
        let tag_id = get_or_create_tag(&db.pool, "hiking").await.unwrap();

        add_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();
        add_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();

        assert_eq!(list_trip_tags(&db.pool, trip_id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn list_trip_tags_is_alphabetical_and_scoped_to_the_trip() {
        let db = TestDb::new().await;
        let a = insert_sample_trip(&db.pool).await;
        let b = insert_sample_trip(&db.pool).await;
        let hiking = get_or_create_tag(&db.pool, "hiking").await.unwrap();
        let alps = get_or_create_tag(&db.pool, "alps").await.unwrap();
        let other = get_or_create_tag(&db.pool, "other").await.unwrap();

        add_trip_tag(&db.pool, a, hiking).await.unwrap();
        add_trip_tag(&db.pool, a, alps).await.unwrap();
        add_trip_tag(&db.pool, b, other).await.unwrap();

        let tags_a = list_trip_tags(&db.pool, a).await.unwrap();
        assert_eq!(
            tags_a.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["alps", "hiking"]
        );
    }

    #[tokio::test]
    async fn remove_trip_tag_unlinks_but_keeps_the_tag_row() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;
        let tag_id = get_or_create_tag(&db.pool, "hiking").await.unwrap();
        add_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();

        let removed = remove_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();
        assert!(removed);

        assert!(list_trip_tags(&db.pool, trip_id).await.unwrap().is_empty());
        // Orphaned tag stays around for reuse/autocomplete (US-33).
        assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn remove_trip_tag_returns_false_when_no_such_link_exists() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;
        let tag_id = get_or_create_tag(&db.pool, "hiking").await.unwrap();

        let removed = remove_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();
        assert!(!removed);
    }

    #[tokio::test]
    async fn deleting_a_trip_cascades_to_its_tag_links_but_not_the_tag() {
        let db = TestDb::new().await;
        let trip_id = insert_sample_trip(&db.pool).await;
        let tag_id = get_or_create_tag(&db.pool, "hiking").await.unwrap();
        add_trip_tag(&db.pool, trip_id, tag_id).await.unwrap();

        sqlx::query("DELETE FROM trip WHERE id = ?")
            .bind(trip_id)
            .execute(&db.pool)
            .await
            .unwrap();

        assert_eq!(list_all_tags(&db.pool).await.unwrap().len(), 1);
    }
}
