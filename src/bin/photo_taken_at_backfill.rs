//! US-62: a one-off CLI giving every photo stored before the archive kept
//! capture times the one its stored copy's EXIF names, so older photos get a
//! caption too. All logic lives in `server::photo_backfill` and is tested
//! there — this file is a thin shell and is not unit-tested, the same policy
//! as the other CLIs. Safe to rerun: photos that already have a capture time
//! are not read again.
//!
//! Usage: `cargo run --bin photo_taken_at_backfill` against the archive's own
//! data directory (the same one the server uses).

use std::process::ExitCode;
use std::sync::Arc;

use trip_archive::config;
use trip_archive::server::storage::{BlobStore, LocalDisk};
use trip_archive::server::{db, paths, photo_backfill};

#[tokio::main]
async fn main() -> ExitCode {
    let data_dir = paths::data_dir();
    let pool = match db::create_pool(&data_dir.join(config::storage::DB_FILENAME)).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("FAILED to open database: {e}");
            return ExitCode::FAILURE;
        }
    };
    let store: Arc<dyn BlobStore> =
        Arc::new(LocalDisk::new(data_dir.join(config::storage::BLOBS_SUBDIR)));

    let summary = match photo_backfill::backfill_taken_at(&pool, &store).await {
        Ok(summary) => summary,
        Err(e) => {
            eprintln!("FAILED: {e}");
            return ExitCode::FAILURE;
        }
    };

    for (key, err) in &summary.unreadable {
        eprintln!("Could not read {key}: {err}");
    }
    println!(
        "{} photo(s) given a capture time; {} name none; {} could not be read.",
        summary.filled,
        summary.without_time,
        summary.unreadable.len()
    );
    if summary.unreadable.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
