use std::sync::Arc;

use sqlx::SqlitePool;

use crate::server::komoot::KomootClient;
use crate::server::storage::BlobStore;

/// Shared server state threaded through Axum handlers via `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    /// Where photo blobs are stored (ADR-0007). `Arc<dyn …>` so the backend is a
    /// swap, not a rewrite, and so the state stays cheap to clone per request.
    pub store: Arc<dyn BlobStore>,
    /// Talks to Komoot (US-22, ADR-0021). `None` when `KOMOOT_EMAIL`/
    /// `KOMOOT_PASSWORD` aren't set — the app still boots and works for
    /// everything except the "Sync now" routes, which report a clear 400
    /// instead of the app refusing to start over an optional integration.
    pub komoot: Option<Arc<dyn KomootClient>>,
}
