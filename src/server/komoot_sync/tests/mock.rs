//! Hand-rolled `KomootClient` test double, shared by `tests.rs` (US-22) and
//! `tests/push.rs` (US-20). Split out purely to keep the parent `tests.rs`
//! under the repo's 500-line cap.

use super::*;

#[derive(Default)]
pub(super) struct MockKomootClient {
    pub(super) tours: Vec<KomootTourSummary>,
    pub(super) gpx: HashMap<String, Vec<u8>>,
    /// Every photo attached to a tour, across all pages — `get_tour_photos`
    /// slices this by `limit`/`page` so a large enough list here genuinely
    /// exercises `list_all_tour_photos`'s pagination loop, the same way the
    /// real (HAL-paginated) API would.
    pub(super) photos: HashMap<String, Vec<KomootPhoto>>,
    pub(super) photo_bytes: HashMap<String, Vec<u8>>,
    pub(super) fail_gpx_for: HashSet<String>,
    pub(super) gpx_calls: Mutex<Vec<String>>,
    /// US-20: `get_tour`'s response per tour id — the tour's *live* current
    /// sport, as `push_one_edit` diffs against before deciding what sport to
    /// push. Independent of `tours` (the `list_tours` fixture) since a push
    /// test cares about one tour's live state, not the full pull-candidate
    /// list.
    pub(super) tour_details: HashMap<String, KomootTourSummary>,
    pub(super) fail_update_tour_for: HashSet<String>,
    /// Every `update_tour` call, in order — asserted on to check exactly
    /// what name/sport a push actually sent.
    pub(super) update_tour_calls: Mutex<Vec<(String, String, String)>>,
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
        // A single page holds every configured tour; any later page is
        // empty, matching the real API's "short page = last page".
        Ok(if page.unwrap_or(0) == 0 {
            self.tours.clone()
        } else {
            Vec::new()
        })
    }

    fn get_tour_gpx(&self, tour_id: &str) -> Result<Vec<u8>, KomootError> {
        self.gpx_calls.lock().unwrap().push(tour_id.to_string());
        if self.fail_gpx_for.contains(tour_id) {
            return Err(KomootError::UnexpectedStatus {
                status: 500,
                body: "boom".to_string(),
            });
        }
        self.gpx
            .get(tour_id)
            .cloned()
            .ok_or_else(|| KomootError::UnexpectedStatus {
                status: 404,
                body: "no gpx configured for this tour in the test".to_string(),
            })
    }

    fn get_tour_photos(
        &self,
        tour_id: &str,
        limit: Option<u32>,
        page: Option<u32>,
    ) -> Result<Vec<KomootPhoto>, KomootError> {
        let all = self.photos.get(tour_id).cloned().unwrap_or_default();
        let limit = limit.unwrap_or(all.len() as u32).max(1) as usize;
        let start = page.unwrap_or(0) as usize * limit;
        Ok(all
            .get(start..)
            .map(|rest| rest.iter().take(limit).cloned().collect())
            .unwrap_or_default())
    }

    fn fetch_photo_bytes(&self, resolved_url: &str) -> Result<Vec<u8>, KomootError> {
        self.photo_bytes
            .get(resolved_url)
            .cloned()
            .ok_or_else(|| KomootError::UnexpectedStatus {
                status: 404,
                body: "no bytes configured for this url in the test".to_string(),
            })
    }

    fn get_tour(&self, tour_id: &str) -> Result<KomootTourSummary, KomootError> {
        self.tour_details
            .get(tour_id)
            .cloned()
            .ok_or_else(|| KomootError::UnexpectedStatus {
                status: 404,
                body: "no tour details configured for this id in the test".to_string(),
            })
    }

    fn update_tour(&self, tour_id: &str, name: &str, sport: &str) -> Result<(), KomootError> {
        self.update_tour_calls.lock().unwrap().push((
            tour_id.to_string(),
            name.to_string(),
            sport.to_string(),
        ));
        if self.fail_update_tour_for.contains(tour_id) {
            return Err(KomootError::UnexpectedStatus {
                status: 500,
                body: "boom".to_string(),
            });
        }
        Ok(())
    }
}

pub(super) fn a_tour(id: &str, name: &str, sport: &str) -> KomootTourSummary {
    KomootTourSummary {
        id: id.to_string(),
        name: name.to_string(),
        sport: sport.to_string(),
        date: "2026-07-11T08:47:52.000Z".to_string(),
        distance: 1000.0,
    }
}

pub(super) fn test_store() -> (Arc<dyn BlobStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let store: Arc<dyn BlobStore> = Arc::new(LocalDisk::new(dir.path().join("blobs")));
    (store, dir)
}
