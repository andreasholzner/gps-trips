//! US-40: pull a consistent backup of the deployed archive into a directory
//! for the borg jobs. All logic lives in `server::backup` and is covered by
//! `tests/us40_backup.rs` and the modules' own tests — this file is a thin
//! shell and is not unit-tested, the same policy as the other CLIs.
//!
//! Usage: `backup [--config <path>]`
//!
//! The archive URL, the backup directory and an optional password command
//! come from the config file — by default `~/.config/trip-archive/backup.toml`;
//! see `server/backup/config.rs` for its shape. Without a password command
//! the password is asked for, unechoed.

use std::path::PathBuf;
use std::process::ExitCode;

use trip_archive::server::backup::client::{self, Options};
use trip_archive::server::backup::config::{self, BackupConfig};

const USAGE: &str = "usage: backup [--config <path>]";

fn parse_args() -> Result<Option<PathBuf>, String> {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next(), args.next()) {
        (None, _, _) => Ok(None),
        (Some("--config"), Some(path), None) => Ok(Some(PathBuf::from(path))),
        _ => Err(USAGE.to_owned()),
    }
}

fn load_config() -> Result<BackupConfig, String> {
    let path = match parse_args()? {
        Some(path) => path,
        None => config::default_path().map_err(|e| e.to_string())?,
    };
    BackupConfig::from_file(&path).map_err(|e| e.to_string())
}

#[tokio::main]
async fn main() -> ExitCode {
    let config = match load_config() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let password = match &config.password_command {
        Some(command) => client::password_from_command(command),
        None => rpassword::prompt_password("Archive password: "),
    };
    let password = match password {
        Ok(password) => password,
        Err(e) => {
            eprintln!("could not read the password: {e}");
            return ExitCode::FAILURE;
        }
    };

    let options = Options {
        url: config.url,
        password,
        target: config.target,
    };
    match client::run(&options).await {
        Ok(report) => {
            // Damage on the server, not a failed backup: the rest arrived.
            for key in &report.missing {
                eprintln!(
                    "error: the archive has no file for photo {key}; it is not in the backup"
                );
            }
            println!(
                "Backed up into {}: {} photo files fetched, {} already held, {} removed.",
                options.target.display(),
                report.fetched,
                report.kept,
                report.removed
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Backup failed: {e}");
            ExitCode::FAILURE
        }
    }
}
