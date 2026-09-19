use serde::{Deserialize, Serialize};

use crate::{ActivityType, Tag, TripKind};

/// One trip as the QMapShack exporter reads it (US-51, ADR-0022's 2026-09-19
/// amendment): everything its change detection compares, tags included —
/// never the geometry, which it fetches per trip, only for the trips it
/// writes. `GET /api/export/trips` answers with every trip, unfiltered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportTrip {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    pub trip_kind: TripKind,
    /// RFC-3339 UTC (ADR-0009), `None` for trips whose GPX had no times.
    pub start_time: Option<String>,
    pub tz_name: Option<String>,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub duration_secs: Option<i64>,
    /// In name order, so the exported item's text is deterministic.
    pub tags: Vec<Tag>,
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trip_survives_the_round_trip_from_server_to_exporter() {
        let trip = ExportTrip {
            id: 7,
            name: "Galdhøpiggen".into(),
            activity_type: ActivityType::Hiking,
            trip_kind: TripKind::Planned,
            start_time: None,
            tz_name: Some("Europe/Oslo".into()),
            distance_m: 12_345.6,
            ascent_m: Some(1_200.0),
            descent_m: None,
            duration_secs: Some(3_600),
            tags: vec![Tag {
                id: 3,
                name: "fjell".into(),
            }],
        };
        let json = serde_json::to_string(&trip).unwrap();
        assert_eq!(serde_json::from_str::<ExportTrip>(&json).unwrap(), trip);
    }
}
