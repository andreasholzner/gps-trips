use std::sync::Arc;

use sqlx::SqlitePool;

use crate::server::storage::BlobStore;

/// Shared server state threaded through Axum handlers via `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    /// Where photo blobs are stored (ADR-0007). `Arc<dyn …>` so the backend is a
    /// swap, not a rewrite, and so the state stays cheap to clone per request.
    pub store: Arc<dyn BlobStore>,
}
