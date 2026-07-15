use std::sync::atomic::{AtomicBool, Ordering};
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
    /// US-26/ADR-0021: true while a "Sync now" run is in flight — guards
    /// `PATCH`/`DELETE /api/trips/:id` and a second concurrent sync against
    /// racing the push phase's read of `edit_pending`/`delete_pending`. A
    /// single in-process flag, since this is a single-process, single-user
    /// app (ADR-0010/0014) — no distributed lock needed. Private — claim it
    /// via [`AppState::try_start_sync`] and read it via
    /// [`AppState::sync_in_progress`], so nothing outside this module can
    /// desync the flag from a live [`SyncGuard`].
    sync_in_progress: Arc<AtomicBool>,
}

/// US-26: a "Sync now" run is currently in flight (or the caller is asking
/// for one while another is), so the request that hit this can't safely
/// proceed. Shared by `handle_sync` (a second concurrent sync), and
/// `handle_edit_trip`/`handle_delete_trip` (racing the push phase's read of
/// `edit_pending`/`delete_pending`) — kept as one constant so their wording
/// can't quietly drift.
pub const SYNC_IN_PROGRESS_MSG: &str = "a Komoot sync is in progress; try again shortly";

impl AppState {
    pub fn new(
        pool: SqlitePool,
        store: Arc<dyn BlobStore>,
        komoot: Option<Arc<dyn KomootClient>>,
    ) -> Self {
        Self {
            pool,
            store,
            komoot,
            sync_in_progress: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Atomically claims the sync flag for the caller. `None` if a sync is
    /// already in flight; otherwise `Some(guard)` — the flag is cleared when
    /// the guard drops, on every return path (success, an early `?` error,
    /// or a panic), so a failed sync (US-25: halt-on-first-failure) can
    /// never leave the app permanently locked out of edits/deletes.
    pub fn try_start_sync(&self) -> Option<SyncGuard> {
        self.sync_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| SyncGuard {
                flag: Arc::clone(&self.sync_in_progress),
            })
    }

    /// Whether a sync is currently in flight (US-26) — checked by the edit
    /// and delete handlers before touching the DB.
    pub fn sync_in_progress(&self) -> bool {
        self.sync_in_progress.load(Ordering::SeqCst)
    }

    /// Test-only direct write to the sync flag, bypassing `try_start_sync`'s
    /// `SyncGuard` — lets a test simulate "a sync is in flight" without
    /// racing a real one (see `tests/us26_sync_blocks_concurrent_edits.rs`).
    /// Gated the same way `komoot::testing`'s `MockKomootClient` is (`test`
    /// or the `test-support` feature `tests/` builds with), so it's
    /// unreachable from any production code path.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_sync_in_progress_for_test(&self, value: bool) {
        self.sync_in_progress.store(value, Ordering::SeqCst);
    }
}

/// RAII handle on the claimed sync flag (see [`AppState::try_start_sync`]).
/// Clears the flag on drop; holds no other state.
pub struct SyncGuard {
    flag: Arc<AtomicBool>,
}

impl Drop for SyncGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::db::testing::TestDb;
    use crate::server::storage::LocalDisk;

    async fn a_state() -> (AppState, tempfile::TempDir) {
        let db = TestDb::new().await;
        let dir = tempfile::tempdir().expect("temp dir");
        let store: Arc<dyn BlobStore> = Arc::new(LocalDisk::new(dir.path().join("blobs")));
        (AppState::new(db.pool, store, None), dir)
    }

    #[tokio::test]
    async fn us26_try_start_sync_rejects_a_second_claim_while_the_first_guard_is_held() {
        let (state, _dir) = a_state().await;

        assert!(!state.sync_in_progress());
        let guard = state.try_start_sync().expect("first claim succeeds");
        assert!(state.sync_in_progress());

        assert!(
            state.try_start_sync().is_none(),
            "a second claim must fail while the first guard is alive"
        );

        drop(guard);
        assert!(!state.sync_in_progress());
        assert!(
            state.try_start_sync().is_some(),
            "the flag must be claimable again once the first guard drops"
        );
    }
}
