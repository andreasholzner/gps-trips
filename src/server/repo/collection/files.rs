//! The files a collection was imported or added from, kept as they were
//! uploaded (US-88, migration 0026).

use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqlitePool, Transaction};

use crate::models::CollectionFile;

/// A file as it was uploaded, under its own name.
pub struct NewCollectionFile<'a> {
    pub file_name: &'a str,
    pub gpx: &'a [u8],
}

/// Keep `files` after the collection's others, in their order.
pub(crate) async fn insert_files(
    tx: &mut Transaction<'_, Sqlite>,
    collection_id: i64,
    files: &[NewCollectionFile<'_>],
) -> Result<(), sqlx::Error> {
    for file in files {
        sqlx::query(
            "INSERT INTO collection_file (collection_id, position, file_name, gpx) \
             VALUES (?1, (SELECT COALESCE(MAX(position), 0) + 1 FROM collection_file \
                          WHERE collection_id = ?1), ?2, ?3)",
        )
        .bind(collection_id)
        .bind(file.file_name)
        .bind(file.gpx)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// A collection's files in the order they were added.
pub async fn list_collection_files(
    pool: &SqlitePool,
    collection_id: i64,
) -> Result<Vec<CollectionFile>, sqlx::Error> {
    sqlx::query(
        "SELECT id, file_name FROM collection_file WHERE collection_id = ? ORDER BY position",
    )
    .bind(collection_id)
    .map(|row: SqliteRow| CollectionFile {
        id: row.get("id"),
        file_name: row.get("file_name"),
    })
    .fetch_all(pool)
    .await
}

/// One kept file's name and bytes, or `None` if there is none.
pub async fn get_collection_file(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    sqlx::query_as("SELECT file_name, gpx FROM collection_file WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}
