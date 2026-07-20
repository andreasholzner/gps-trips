//! US-23: a one-off CLI to bulk-import every historical Komoot tour (incl.
//! photos) not yet linked in the archive, through the same pipeline "Sync
//! now" uses (ADR-0021). Reuses `komoot_sync::backfill`, which composes the
//! already-tested `list_sync_candidates` (anti-join dedup, safe to rerun
//! after an interruption) and `sync_selected_tours` (transactional per-tour
//! import, halts on first failure). No web UI interaction. Real-API
//! interaction is this binary's own acceptance check, the same as
//! `komoot_check.rs` (US-27) — only pure/composed logic gets automated
//! tests, not this file.
//!
//! Usage: `KOMOOT_EMAIL=... KOMOOT_PASSWORD=... cargo run --bin komoot_backfill \
//!         [--interactive] [--planned] [--limit N] [--debug|-d]`
//!
//! `--planned` bulk-imports Komoot's *planned* routes (US-29) instead of
//! recorded tours; one run pulls one kind. Without it, recorded tours are
//! imported (the original behaviour).

use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::Arc;

use trip_archive::config;
use trip_archive::models::TripKind;
use trip_archive::server::komoot::{
    KomootClient, KomootError, KomootHttpClient, KomootPhoto, KomootTourSummary,
};
use trip_archive::server::storage::{BlobStore, LocalDisk};
use trip_archive::server::{db, komoot_sync, paths};

struct Args {
    interactive: bool,
    debug: bool,
    limit: Option<usize>,
    kind: TripKind,
}

fn parse_args() -> Result<Args, String> {
    let mut interactive = false;
    let mut debug = false;
    let mut limit = None;
    let mut kind = TripKind::Recorded;

    let mut raw = std::env::args().skip(1);
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--interactive" => interactive = true,
            "--debug" | "-d" => debug = true,
            "--planned" => kind = TripKind::Planned,
            "--limit" => {
                let value = raw.next().ok_or("--limit requires a number")?;
                limit = Some(value.parse::<usize>().map_err(|_| {
                    format!("--limit value must be a non-negative integer, got {value:?}")
                })?);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(Args {
        interactive,
        debug,
        limit,
        kind,
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

    let http_client = KomootHttpClient::new(email, password, args.debug);
    let client: Arc<dyn KomootClient> = if args.interactive {
        Arc::new(InteractiveKomootClient { inner: http_client })
    } else {
        Arc::new(http_client)
    };

    let data_dir = paths::data_dir();
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        eprintln!("FAILED to create data directory {data_dir:?}: {e}");
        return ExitCode::FAILURE;
    }
    let pool = match db::create_pool(&data_dir.join(config::storage::DB_FILENAME)).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("FAILED to open database: {e}");
            return ExitCode::FAILURE;
        }
    };
    let store: Arc<dyn BlobStore> =
        Arc::new(LocalDisk::new(data_dir.join(config::storage::BLOBS_SUBDIR)));

    let noun = match args.kind {
        TripKind::Recorded => "tour",
        TripKind::Planned => "planned route",
    };
    if let Some(limit) = args.limit {
        println!("Backfilling up to {limit} not-yet-linked Komoot {noun}(s)...");
    } else {
        println!("Backfilling every not-yet-linked Komoot {noun}...");
    }

    let summary = match komoot_sync::backfill(&pool, &store, client, args.limit, args.kind).await {
        Ok(summary) => summary,
        Err(e) => {
            eprintln!("FAILED: {e}");
            return ExitCode::FAILURE;
        }
    };

    for (tour_id, trip_id) in &summary.imported {
        println!("Imported tour {tour_id} as trip {trip_id}");
    }

    match &summary.failed {
        Some((tour_id, msg)) => {
            eprintln!(
                "FAILED at tour {tour_id}: {msg}\n{} tour(s) imported before the failure; rerun to resume (already-imported tours are skipped).",
                summary.imported.len()
            );
            ExitCode::FAILURE
        }
        None => {
            println!("Done: {} tour(s) imported.", summary.imported.len());
            ExitCode::SUCCESS
        }
    }
}

/// `--interactive`: wraps a real `KomootClient`, asking for confirmation on
/// stdin before delegating each call — "mainly for testing against the real
/// API" (US-23's acceptance criteria). Declining halts the run the same way
/// a real failure would (US-25's halt-on-first-failure semantics).
struct InteractiveKomootClient {
    inner: KomootHttpClient,
}

impl InteractiveKomootClient {
    fn confirm(&self, description: &str) -> Result<(), KomootError> {
        print!("Komoot request: {description}? [y/N] ");
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).ok();
        if answer.trim().eq_ignore_ascii_case("y") {
            Ok(())
        } else {
            Err(KomootError::Declined(description.to_string()))
        }
    }
}

impl KomootClient for InteractiveKomootClient {
    fn login(&self) -> Result<String, KomootError> {
        self.confirm("login")?;
        self.inner.login()
    }

    fn list_tours(
        &self,
        username: &str,
        limit: Option<u32>,
        page: Option<u32>,
    ) -> Result<Vec<KomootTourSummary>, KomootError> {
        self.confirm(&format!("list tours (page {})", page.unwrap_or(0)))?;
        self.inner.list_tours(username, limit, page)
    }

    fn list_planned_tours(
        &self,
        username: &str,
        limit: Option<u32>,
        page: Option<u32>,
    ) -> Result<Vec<KomootTourSummary>, KomootError> {
        self.confirm(&format!("list planned tours (page {})", page.unwrap_or(0)))?;
        self.inner.list_planned_tours(username, limit, page)
    }

    fn get_tour_gpx(&self, tour_id: &str) -> Result<Vec<u8>, KomootError> {
        self.confirm(&format!("fetch GPX for tour {tour_id}"))?;
        self.inner.get_tour_gpx(tour_id)
    }

    fn get_tour_photos(
        &self,
        tour_id: &str,
        limit: Option<u32>,
        page: Option<u32>,
    ) -> Result<Vec<KomootPhoto>, KomootError> {
        self.confirm(&format!(
            "list photos for tour {tour_id} (page {})",
            page.unwrap_or(0)
        ))?;
        self.inner.get_tour_photos(tour_id, limit, page)
    }

    fn fetch_photo_bytes(&self, resolved_url: &str) -> Result<Vec<u8>, KomootError> {
        self.confirm(&format!("fetch photo bytes from {resolved_url}"))?;
        self.inner.fetch_photo_bytes(resolved_url)
    }

    fn get_tour(&self, tour_id: &str) -> Result<KomootTourSummary, KomootError> {
        self.confirm(&format!("fetch tour {tour_id}"))?;
        self.inner.get_tour(tour_id)
    }

    fn update_tour(&self, tour_id: &str, name: &str, sport: &str) -> Result<(), KomootError> {
        self.confirm(&format!("update tour {tour_id}"))?;
        self.inner.update_tour(tour_id, name, sport)
    }

    fn delete_tour(&self, tour_id: &str) -> Result<(), KomootError> {
        self.confirm(&format!("delete tour {tour_id}"))?;
        self.inner.delete_tour(tour_id)
    }
}
