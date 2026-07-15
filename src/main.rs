use std::sync::Arc;

use tokio::net::TcpListener;
use trip_archive::config;
use trip_archive::server;
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

    let pool = server::db::create_pool(&data_dir.join(config::storage::DB_FILENAME)).await?;
    let store: Arc<dyn BlobStore> =
        Arc::new(LocalDisk::new(data_dir.join(config::storage::BLOBS_SUBDIR)));
    let komoot = komoot_client_from_env();
    let app = server::http::router(server::state::AppState::new(pool, store, komoot));

    let addr = config::server::BIND_ADDR;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Trip Archive listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
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
