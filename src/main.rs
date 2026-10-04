use std::sync::Arc;

use tokio::net::TcpListener;
use trip_archive::config;
use trip_archive::server;
use trip_archive::server::auth::{Auth, Salt};
use trip_archive::server::komoot::{KomootClient, KomootHttpClient};
use trip_archive::server::storage::{BlobStore, LocalDisk};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "trip_archive=info".into()),
        )
        .init();

    let data_dir = server::paths::data_dir();
    std::fs::create_dir_all(&data_dir)?;

    // Before anything is served: an archive on the public internet
    // (ADR-0023) that boots without its shared password is the failure US-19
    // exists to prevent, so a missing or empty one stops the boot here rather
    // than opening the archive (US-48). There is no development exemption.
    // The key is derived under the salt the data directory keeps (US-55).
    let salt = Salt::load_or_create(&data_dir.join(config::auth::SALT_FILENAME))?;
    let auth = Auth::from_env(&salt)?;
    let addr = server::paths::bind_addr()?;

    let db_path = data_dir.join(config::storage::DB_FILENAME);
    let pool = server::db::create_pool(&db_path).await?;
    // US-77: trips imported before moving time was stored get theirs, once.
    let filled = server::repo::backfill_moving_secs(&pool).await?;
    if filled > 0 {
        tracing::info!("Worked out the moving time of {filled} trips");
    }
    let store: Arc<dyn BlobStore> =
        Arc::new(LocalDisk::new(data_dir.join(config::storage::BLOBS_SUBDIR)));
    let komoot = komoot_client_from_env();
    let places = places_from(&data_dir.join(config::storage::PLACES_DB_FILENAME)).await;
    let ground = ground_from(&data_dir.join(config::storage::GROUND_DB_FILENAME)).await;
    let state = server::state::AppState::new(pool.clone(), store, komoot, auth)
        .with_places(places)
        .with_ground(ground);
    let access_log = state.access_log.clone();
    let app = server::http::router(state);

    let listener = TcpListener::bind(addr).await?;
    // The bound address rather than the configured one: they differ for port 0.
    tracing::info!(
        "Trip Archive listening on http://{}",
        listener.local_addr()?
    );
    // With the peer's address, for the access log of a run with no proxy in
    // front of it (US-70).
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(stop_signal())
    .await?;

    // Every request has finished; the last of their records go in before
    // the database closes (US-70), checkpointing the WAL (US-47).
    access_log.flush().await;
    server::db::close(pool, &db_path).await?;
    tracing::info!("Trip Archive stopped");
    Ok(())
}

/// Resolves on SIGTERM or SIGINT (US-47): the first is what the platform
/// sends when it stops the machine (`kill_signal` in fly.toml), the second
/// is Ctrl-C on the laptop. Either stops the server from accepting, lets the
/// requests in flight finish, and then lets `main` close the database.
async fn stop_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut terminate = signal(SignalKind::terminate()).expect("SIGTERM handler");
    let mut interrupt = signal(SignalKind::interrupt()).expect("SIGINT handler");
    tokio::select! {
        _ = terminate.recv() => {}
        _ = interrupt.recv() => {}
    }
    tracing::info!("Stop signal received; finishing requests in flight");
}

/// Build the Komoot client (US-22, ADR-0021) from `KOMOOT_EMAIL`/
/// `KOMOOT_PASSWORD` if both are set; `None` (not a hard failure) if either
/// is missing, so running without Komoot credentials configured still boots
/// the rest of the app.
fn komoot_client_from_env() -> Option<Arc<dyn KomootClient>> {
    let email = std::env::var(config::komoot::EMAIL_ENV_VAR).ok()?;
    let password = std::env::var(config::komoot::PASSWORD_ENV_VAR).ok()?;
    Some(Arc::new(KomootHttpClient::new(email, password, false)))
}

/// The place-name database (US-74, ADR-0027) at `path`, if there is one.
/// Optional, so neither a missing nor an unreadable one stops the archive:
/// names are then suggested as before, from the GPX.
async fn places_from(path: &std::path::Path) -> Option<server::places::PlaceDb> {
    match server::places::PlaceDb::open(path).await {
        Ok(Some(places)) => {
            tracing::info!("Suggesting names from the places in {}", path.display());
            Some(places)
        }
        Ok(None) => {
            tracing::info!("No place database at {}", path.display());
            None
        }
        Err(e) => {
            tracing::warn!(
                "The place database at {} cannot be read: {e}",
                path.display()
            );
            None
        }
    }
}

/// The ground database (US-76, ADR-0027) at `path`, if there is one.
/// Optional in the same way: without it no activity type is suggested.
async fn ground_from(path: &std::path::Path) -> Option<server::ground::GroundDb> {
    match server::ground::GroundDb::open(path).await {
        Ok(Some(ground)) => {
            tracing::info!(
                "Suggesting activity types from the ground in {}",
                path.display()
            );
            Some(ground)
        }
        Ok(None) => {
            tracing::info!("No ground database at {}", path.display());
            None
        }
        Err(e) => {
            tracing::warn!(
                "The ground database at {} cannot be read: {e}",
                path.display()
            );
            None
        }
    }
}
