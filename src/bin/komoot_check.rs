//! US-27: a minimal, standalone check that the reverse-engineered Komoot API
//! still works — no DB, `BlobStore`, or import pipeline involved (ADR-0021).
//!
//! Usage: `KOMOOT_EMAIL=... KOMOOT_PASSWORD=... cargo run --bin komoot_check [--debug|-d]`

use std::process::ExitCode;

use trip_archive::config;
use trip_archive::server::komoot::{KomootClient, KomootHttpClient};

fn main() -> ExitCode {
    let debug = std::env::args()
        .skip(1)
        .any(|a| a == "--debug" || a == "-d");

    let email = match std::env::var(config::komoot::EMAIL_ENV_VAR) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("FAILED: {} is not set.", config::komoot::EMAIL_ENV_VAR);
            return ExitCode::FAILURE;
        }
    };
    let password = match std::env::var(config::komoot::PASSWORD_ENV_VAR) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("FAILED: {} is not set.", config::komoot::PASSWORD_ENV_VAR);
            return ExitCode::FAILURE;
        }
    };

    let client = KomootHttpClient::new(email, password, debug);

    let username = match client.login() {
        Ok(username) => username,
        Err(e) => {
            eprintln!("FAILED at login: {e}");
            return ExitCode::FAILURE;
        }
    };

    let tours = match client.list_tours(&username, Some(1)) {
        Ok(tours) => tours,
        Err(e) => {
            eprintln!("FAILED at list_tours: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "OK: logged in as {username}; list_tours succeeded ({} of up to 1 requested).",
        tours.len()
    );
    ExitCode::SUCCESS
}
