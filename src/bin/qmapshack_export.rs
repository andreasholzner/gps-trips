//! US-36/US-37/US-51: one-off CLI reconciling the whole trip archive into a
//! QMapShack database (ADR-0022), run manually or from the owner's own
//! scheduler (cron), never from inside the app (ADR-0014). It reads the
//! archive over its HTTP API, never a database file. All logic lives in
//! `qmapshack::run_export` and is covered by `tests/it/us36_qmapshack_export.rs`,
//! `tests/it/us37_qmapshack_resync.rs` and `tests/it/us51_remote_export.rs` — this
//! file is a thin shell and is not unit-tested, the same policy as the other
//! CLIs.
//!
//! Usage: `qmapshack_export [--config <path>] [--debug|-d]`
//!
//! The archive URL, an optional password command, the target database path
//! and the folder mapping come from the config file — by default
//! `~/.config/trip-archive/qmapshack_export.toml`; see
//! `server/qmapshack/config.rs` for its shape. Without a password command the
//! password is asked for, unechoed.

use std::path::PathBuf;
use std::process::ExitCode;

use trip_archive::server::archive_client;
use trip_archive::server::qmapshack::{self, config::ExportConfig};

const USAGE: &str = "usage: qmapshack_export [--config <path>] [--debug|-d]";

struct Args {
    config_path: Option<PathBuf>,
    debug: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut config_path = None;
    let mut debug = false;

    let mut raw = std::env::args().skip(1);
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--debug" | "-d" => debug = true,
            "--config" if config_path.is_none() => {
                config_path = Some(PathBuf::from(raw.next().ok_or(USAGE)?));
            }
            _ => return Err(USAGE.to_owned()),
        }
    }
    Ok(Args { config_path, debug })
}

fn load_config(path: Option<PathBuf>) -> Result<ExportConfig, String> {
    let path = match path {
        Some(path) => path,
        None => ExportConfig::default_path().map_err(|e| e.to_string())?,
    };
    ExportConfig::load(&path).map_err(|e| format!("FAILED to load config {}: {e}", path.display()))
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}");
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

    let cfg = match load_config(args.config_path) {
        Ok(cfg) => cfg,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let password = match archive_client::read_password(cfg.password_command.as_deref()) {
        Ok(password) => password,
        Err(e) => {
            eprintln!("FAILED: could not read the password: {e}");
            return ExitCode::FAILURE;
        }
    };

    let outcome = match qmapshack::run_export(&cfg.url, &password, &cfg).await {
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
