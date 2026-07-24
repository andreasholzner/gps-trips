//! US-36/US-37: one-off CLI reconciling the whole trip archive into a
//! QMapShack database (ADR-0022), run manually or from the owner's own
//! scheduler (cron), never from inside the app (ADR-0014). All logic lives
//! in `qmapshack::run_export` and is covered by
//! `tests/us36_qmapshack_export.rs` and `tests/us37_qmapshack_resync.rs` —
//! this file is a thin shell and is not unit-tested, the same policy as
//! `komoot_backfill.rs`.
//!
//! Usage: `cargo run --bin qmapshack_export -- <config.toml> [--debug|-d]`
//!
//! The config file holds the target database path and the folder-mapping
//! template — see `server/qmapshack/config.rs` for the schema.

use std::path::PathBuf;
use std::process::ExitCode;

use trip_archive::config;
use trip_archive::server::qmapshack::{self, config::ExportConfig};
use trip_archive::server::{db, paths};

struct Args {
    config_path: PathBuf,
    debug: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut config_path = None;
    let mut debug = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--debug" | "-d" => debug = true,
            other if other.starts_with('-') => return Err(format!("unknown argument: {other}")),
            other => {
                if config_path.replace(PathBuf::from(other)).is_some() {
                    return Err("expected exactly one config file path".to_string());
                }
            }
        }
    }

    Ok(Args {
        config_path: config_path.ok_or("usage: qmapshack_export <config.toml> [--debug|-d]")?,
        debug,
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("FAILED: {e}");
            return ExitCode::FAILURE;
        }
    };

    // ADR-0022 wants each per-item decision logged; binaries don't inherit
    // main.rs's tracing setup, so initialize it here.
    let default_filter = if args.debug {
        "trip_archive=debug"
    } else {
        "trip_archive=info"
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .init();

    let cfg = match ExportConfig::load(&args.config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("FAILED to load config {}: {e}", args.config_path.display());
            return ExitCode::FAILURE;
        }
    };

    // Unlike the import-direction binaries, refuse to run against a missing
    // archive: `create_pool` would silently create an empty one and this
    // exporter would "successfully" export zero trips — exactly the
    // misconfiguration (wrong TRIP_ARCHIVE_DATA_DIR, cron cwd) the owner
    // relies on the exit code to catch (ADR-0022 failure visibility).
    let data_dir = paths::data_dir();
    let db_path = data_dir.join(config::storage::DB_FILENAME);
    if !db_path.exists() {
        eprintln!(
            "FAILED: archive database {} does not exist — nothing to export. \
             Is {} set correctly?",
            db_path.display(),
            trip_archive::config::storage::DATA_DIR_ENV_VAR
        );
        return ExitCode::FAILURE;
    }
    let pool = match db::create_pool(&db_path).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("FAILED to open archive database: {e}");
            return ExitCode::FAILURE;
        }
    };

    let outcome = match qmapshack::run_export(&pool, &cfg).await {
        Ok(outcome) => outcome,
        Err(e) => {
            eprintln!("FAILED: {e:#}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "Done: {} inserted, {} updated, {} removed, {} skipped, {} failed.",
        outcome.inserted, outcome.updated, outcome.removed, outcome.skipped, outcome.failed
    );
    if outcome.failed > 0 {
        eprintln!(
            "FAILED to export {} trip(s) — see the log above; rerunning retries them.",
            outcome.failed
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
