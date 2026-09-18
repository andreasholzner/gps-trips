//! US-40: a consistent backup of the deployed archive.
//!
//! The server's half is one route, a snapshot of the database. The photos
//! need nothing new: they never change once stored, the snapshot names every
//! one of them, and `/media/*path` already serves them. The laptop's half —
//! fetching the snapshot and the photos it names into a backup directory —
//! is [`client`].

pub mod client;
pub mod config;

use axum::{
    body::Body,
    extract::State,
    http::header,
    response::{IntoResponse, Response},
};
use tokio_util::io::ReaderStream;

use crate::config::storage::DB_FILENAME;
use crate::server::{error::AppError, state::AppState};

/// The media type of a snapshot, as registered for SQLite database files.
pub const SNAPSHOT_CONTENT_TYPE: &str = "application/vnd.sqlite3";

/// GET `/api/backup/database` — a snapshot of the whole database (US-40).
///
/// `VACUUM INTO` rather than a copy of the file: the live database is in WAL
/// mode, so its file alone may miss committed writes, and a copy taken during
/// a write can be torn. `VACUUM INTO` reads one consistent state and writes it
/// out as a self-contained file. That file goes to the system temp directory,
/// not the volume, and is unlinked as soon as it is open — the stream keeps
/// reading it, and nothing is left behind however the response ends.
pub async fn handle_database_snapshot(State(state): State<AppState>) -> Result<Response, AppError> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(DB_FILENAME);
    let target = path
        .to_str()
        .ok_or_else(|| AppError::Internal("temp path is not UTF-8".into()))?;
    sqlx::query("VACUUM INTO ?")
        .bind(target)
        .execute(&state.pool)
        .await?;

    let file = tokio::fs::File::open(&path).await?;
    let len = file.metadata().await?.len();
    drop(dir);

    Ok((
        [
            (header::CONTENT_TYPE, SNAPSHOT_CONTENT_TYPE.to_owned()),
            (header::CONTENT_LENGTH, len.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{DB_FILENAME}\""),
            ),
        ],
        Body::from_stream(ReaderStream::new(file)),
    )
        .into_response())
}
