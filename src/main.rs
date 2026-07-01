use std::sync::Arc;

use tokio::net::TcpListener;
use trip_archive::server;
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

    let pool = server::db::create_pool(&data_dir.join("trip-archive.db")).await?;
    let store: Arc<dyn BlobStore> = Arc::new(LocalDisk::new(data_dir.join("photos")));
    let app = server::http::router(server::state::AppState { pool, store });

    let addr = "127.0.0.1:3000";
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Trip Archive listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
