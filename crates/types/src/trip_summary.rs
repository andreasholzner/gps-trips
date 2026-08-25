use serde::{Deserialize, Serialize};

use crate::{ActivityType, KomootPrivacy, TripKind};

/// A lightweight trip row for the list view (US-6). Holds only the summary
/// fields shown in the list — never the track geometry (ADR-0003), so the list
/// query stays cheap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TripSummary {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    pub start_time: Option<String>,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub duration_secs: Option<i64>,
    /// Recorded vs. planned (US-32); always `Recorded` until US-31 gives the
    /// owner a way to import a trip as planned.
    pub trip_kind: TripKind,
    /// The linked Komoot tour's privacy (US-35), joined in from
    /// `trip_komoot_link`. `None` for a trip that never came from Komoot —
    /// and for a linked one whose privacy no sync has read yet; the list page
    /// shows a dash either way.
    pub privacy_status: Option<KomootPrivacy>,
}
