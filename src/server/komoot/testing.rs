//! A hand-rolled `KomootClient` test double, `pub` (gated by `test-support`
//! as well as `test`, mirroring `location::fixtures` and `thumbnail`'s
//! fixture builders) so both this crate's own unit tests *and*
//! `tests/us25_sync_halts_on_failure.rs`'s HTTP-level acceptance tests can
//! drive the same mock through the real `AppState`/router — see the
//! `[dev-dependencies]` entry in `Cargo.toml` that enables `test-support`
//! for `tests/`.
//!
//! Deliberately separate from `komoot_sync/tests/mock.rs`'s
//! module-private mock: that one is purpose-built for `komoot_sync.rs`'s
//! own unit tests (finer-grained per-call fixtures); this one is shaped
//! for exercising `http.rs`'s `handle_sync` end to end, where the only
//! thing that matters is *which* Komoot call fails and whether later calls
//! happen at all.

use std::collections::HashMap;
use std::sync::Mutex;

use super::{KomootClient, KomootError, KomootPhoto, KomootTourSummary, TourUpdate};

/// Every Komoot call this mock recorded, in order — used to assert that a
/// halted sync genuinely never attempted a later item (US-25), not just
/// that it reported a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedCall {
    /// The tour id plus the `status` (privacy) the push sent, if any —
    /// `None` means the push deliberately omitted the field, leaving
    /// Komoot's own privacy untouched.
    UpdateTour(String, Option<String>),
    DeleteTour(String),
    GetTourGpx(String),
}

/// A `KomootClient` double whose behaviour is entirely driven by which
/// tour ids are listed in its `fail_*` sets. Every tour not named there
/// succeeds; every call is appended to `calls` regardless of outcome, so
/// tests can assert on exactly what was (and wasn't) attempted.
#[derive(Default)]
pub struct MockKomootClient {
    pub tours: Vec<KomootTourSummary>,
    /// Planned routes returned by `list_planned_tours` (US-29), kept separate
    /// from `tours` (recorded) so a test can configure each independently.
    pub planned_tours: Vec<KomootTourSummary>,
    pub calls: Mutex<Vec<RecordedCall>>,
    pub fail_update_tour_for: std::collections::HashSet<String>,
    pub fail_delete_tour_for: std::collections::HashSet<String>,
    pub fail_get_tour_gpx_for: std::collections::HashSet<String>,
    /// Komoot `status` (privacy) values that replace a configured tour's own,
    /// as of the next call — how a test says "the owner changed this tour's
    /// privacy inside Komoot after it was imported" (US-35). Behind a
    /// `Mutex` because the client is shared as `Arc<dyn KomootClient>` by the
    /// time a test wants to change it.
    pub status_overrides: Mutex<HashMap<String, String>>,
}

impl MockKomootClient {
    /// Make Komoot report a different privacy for `tour_id` from now on.
    pub fn set_tour_status(&self, tour_id: &str, status: &str) {
        self.status_overrides
            .lock()
            .unwrap()
            .insert(tour_id.to_string(), status.to_string());
    }

    /// A tour as Komoot would report it right now, i.e. with any override
    /// applied — every listing/detail response goes through this.
    fn as_reported(&self, tour: &KomootTourSummary) -> KomootTourSummary {
        let mut tour = tour.clone();
        if let Some(status) = self.status_overrides.lock().unwrap().get(&tour.id) {
            tour.status = status.clone();
        }
        tour
    }
}

fn boom() -> KomootError {
    KomootError::UnexpectedStatus {
        status: 500,
        body: "boom".to_string(),
    }
}

impl KomootClient for MockKomootClient {
    fn login(&self) -> Result<String, KomootError> {
        Ok("testuser".to_string())
    }

    fn list_tours(
        &self,
        _username: &str,
        _limit: Option<u32>,
        page: Option<u32>,
    ) -> Result<Vec<KomootTourSummary>, KomootError> {
        Ok(if page.unwrap_or(0) == 0 {
            self.tours.iter().map(|t| self.as_reported(t)).collect()
        } else {
            Vec::new()
        })
    }

    fn list_planned_tours(
        &self,
        _username: &str,
        _limit: Option<u32>,
        page: Option<u32>,
    ) -> Result<Vec<KomootTourSummary>, KomootError> {
        Ok(if page.unwrap_or(0) == 0 {
            self.planned_tours
                .iter()
                .map(|t| self.as_reported(t))
                .collect()
        } else {
            Vec::new()
        })
    }

    fn get_tour_gpx(&self, tour_id: &str) -> Result<Vec<u8>, KomootError> {
        self.calls
            .lock()
            .unwrap()
            .push(RecordedCall::GetTourGpx(tour_id.to_string()));
        if self.fail_get_tour_gpx_for.contains(tour_id) {
            return Err(boom());
        }
        Ok(include_bytes!("../../../tests/fixtures/sample.gpx").to_vec())
    }

    fn get_tour_photos(
        &self,
        _tour_id: &str,
        _limit: Option<u32>,
        _page: Option<u32>,
    ) -> Result<Vec<KomootPhoto>, KomootError> {
        Ok(Vec::new())
    }

    fn fetch_photo_bytes(&self, _resolved_url: &str) -> Result<Vec<u8>, KomootError> {
        Err(boom())
    }

    fn get_tour(&self, tour_id: &str) -> Result<KomootTourSummary, KomootError> {
        self.tours
            .iter()
            .find(|t| t.id == tour_id)
            .map(|t| self.as_reported(t))
            .ok_or_else(|| KomootError::UnexpectedStatus {
                status: 404,
                body: "no tour configured for this id in the test".to_string(),
            })
    }

    fn update_tour(&self, tour_id: &str, update: &TourUpdate<'_>) -> Result<(), KomootError> {
        self.calls.lock().unwrap().push(RecordedCall::UpdateTour(
            tour_id.to_string(),
            update.status.map(str::to_string),
        ));
        if self.fail_update_tour_for.contains(tour_id) {
            return Err(boom());
        }
        Ok(())
    }

    fn delete_tour(&self, tour_id: &str) -> Result<(), KomootError> {
        self.calls
            .lock()
            .unwrap()
            .push(RecordedCall::DeleteTour(tour_id.to_string()));
        if self.fail_delete_tour_for.contains(tour_id) {
            return Err(boom());
        }
        Ok(())
    }
}

/// A minimal `KomootTourSummary` fixture — only the fields the sync tests
/// actually branch on need real values.
pub fn a_tour(id: &str, name: &str, sport: &str) -> KomootTourSummary {
    a_tour_with_status(id, name, sport, "private")
}

/// A tour fixture whose Komoot privacy (`status`) the test cares about — the
/// value the pull mirrors into `trip_komoot_link.privacy_status`.
pub fn a_tour_with_status(id: &str, name: &str, sport: &str, status: &str) -> KomootTourSummary {
    KomootTourSummary {
        id: id.to_string(),
        name: name.to_string(),
        sport: sport.to_string(),
        date: "2026-07-11T08:47:52.000Z".to_string(),
        distance: 1000.0,
        status: status.to_string(),
    }
}
