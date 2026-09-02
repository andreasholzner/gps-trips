use serde::{Deserialize, Serialize};

use crate::TripKind;

/// A not-yet-synced Komoot tour, as offered to the owner on the "Sync now"
/// review screen (US-22/US-29).
///
/// Every field is read straight off Komoot's own tour listing — no extra
/// per-tour call is needed to render the screen (`docs/komoot-api.md`) — and
/// travels to the client as-is, so this is a response type in
/// [ADR-0015](../../../docs/adr/0015-db-model-response-type-separation.md)'s
/// sense: no stored record grows a field for it, and nothing here needs the
/// server to compute it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncCandidate {
    pub tour_id: String,
    pub name: String,
    pub sport: String,
    pub date: String,
    pub distance_m: f64,
    /// Recorded or planned (US-29) — decides which list tab the imported trip
    /// lands on, and is shown on the review screen so the owner knows which
    /// kind they are pulling.
    pub kind: TripKind,
}

/// What `GET /api/komoot/sync` answers with: what a "Sync now" run would do
/// if the owner pressed the button right now.
///
/// The two pending counts are what the *push* phases would send (US-20's
/// edits, US-24's deletes), which happen before the pull whether or not a
/// single tour is ticked — so the screen can say what is about to leave the
/// archive, not only what is about to arrive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncCandidates {
    pub candidates: Vec<SyncCandidate>,
    /// US-20: trips whose name/activity_type edit is queued for Komoot.
    pub pending_edits: i64,
    /// US-24: link rows whose tour is queued for deletion on Komoot.
    pub pending_deletes: i64,
}

/// One tour the owner ticked on the review screen: its id plus the `kind` the
/// screen already knew it was (US-29).
///
/// Carrying the kind in the request lets the pull list only the endpoint(s)
/// the selection actually spans, instead of always paging both — a flat id
/// list would drop it and force a redundant fetch of the other endpoint every
/// sync.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectedTour {
    pub tour_id: String,
    pub kind: TripKind,
}

/// The `POST /api/komoot/sync` body: the tours the owner checked, in
/// submission order (ADR-0008).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SyncRequest {
    pub tours: Vec<SelectedTour>,
}

/// Which half of a sync run a failure belongs to (US-25) — a closed set, so
/// an enum rather than a bare string (ADR-0018). Serialized as `"push"` /
/// `"pull"`, the values the review screen and every existing acceptance test
/// already read.
///
/// Both push phases report as `Push`: the owner's question is "did my archive
/// fail to reach Komoot, or did Komoot fail to reach my archive", and the
/// specific step is named in `failed_msg` (`push_pending_deletes` prefixes
/// its own errors with `"delete tour: "`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPhase {
    Push,
    Pull,
}

impl SyncPhase {
    /// The verb the screen uses when naming what failed: "Failed to push
    /// tour 123" / "Failed to pull tour 123".
    pub fn verb(&self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::Pull => "pull",
        }
    }

    /// Every variant, exhaustively matched in this file's tests so a future
    /// phase cannot silently drift out of this list.
    pub const ALL: [SyncPhase; 2] = [Self::Push, Self::Pull];
}

/// What `POST /api/komoot/sync` answers with: how many pending edits and
/// deletes were pushed and how many tours were pulled, plus the one trip or
/// tour that halted the run, if any.
///
/// A halt is not an error status: the phases before it did real work, and the
/// owner needs both halves of that story (US-25). So the run reports `200`
/// with `failed_tour` set, rather than a status that would throw the counts
/// away.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SyncResponse {
    pub pushed: usize,
    /// US-24: tours deleted on Komoot this run.
    pub deleted: usize,
    pub imported: usize,
    pub failed_tour: Option<String>,
    pub failed_msg: Option<String>,
    pub failed_phase: Option<SyncPhase>,
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phase_travels_as_the_word_the_screen_shows() {
        // The wire values are load-bearing: `us25_sync_halts_on_failure.rs`
        // asserts on them, and the review screen reads them back.
        for phase in SyncPhase::ALL {
            let json = serde_json::to_string(&phase).unwrap();
            assert_eq!(json, format!("\"{}\"", phase.verb()));
            assert_eq!(
                serde_json::from_str::<SyncPhase>(&json).unwrap(),
                phase,
                "a phase must survive the round trip it makes on every sync"
            );
        }
    }

    #[test]
    fn all_lists_every_phase_exactly_once() {
        // Exhaustive match, no wildcard arm: adding a variant without
        // updating `ALL` fails to compile.
        for phase in SyncPhase::ALL {
            match phase {
                SyncPhase::Push | SyncPhase::Pull => {}
            }
        }
        assert_eq!(SyncPhase::ALL.len(), 2);
    }

    #[test]
    fn a_run_that_halted_nothing_reports_no_failure() {
        let json = serde_json::to_string(&SyncResponse {
            imported: 2,
            ..Default::default()
        })
        .unwrap();

        let back: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(back["imported"], 2);
        assert!(back["failed_tour"].is_null());
        assert!(back["failed_phase"].is_null());
    }

    #[test]
    fn a_selection_carries_the_kind_the_screen_knew_it_was() {
        // US-29: the pull lists only the endpoint(s) the selection spans, and
        // this is the field it decides that from.
        let json = r#"{"tours":[{"tour_id":"111","kind":"planned"}]}"#;
        let request: SyncRequest = serde_json::from_str(json).unwrap();

        assert_eq!(
            request.tours,
            vec![SelectedTour {
                tour_id: "111".to_string(),
                kind: TripKind::Planned,
            }]
        );
    }
}
