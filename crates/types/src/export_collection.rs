use serde::{Deserialize, Serialize};

use crate::ActivityType;

/// One guide-book collection as the QMapShack exporter reads it (US-89,
/// ADR-0022's 2026-10-11 amendment): what its change detection compares for
/// each track, and every waypoint whole — never a track's geometry, which it
/// fetches per track, only for the tracks it writes.
/// `GET /api/export/collections` answers with every collection, unfiltered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportCollection {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// RFC-3339 UTC (ADR-0009).
    pub created_at: String,
    /// In file order.
    pub tracks: Vec<ExportCollectionTrack>,
    /// In file order.
    pub waypoints: Vec<ExportWaypoint>,
}

/// One of an exported collection's tracks: its cheap columns. Its stats and
/// bounds also tell a geometry replaced in place (US-88).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportCollectionTrack {
    pub id: i64,
    pub name: String,
    /// The activity the track goes by: its own, else its collection's, else
    /// `Unknown`.
    pub activity_type: ActivityType,
    pub distance_m: f64,
    pub ascent_m: f64,
    pub descent_m: f64,
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_lat: f64,
    pub max_lon: f64,
}

/// One of an exported collection's waypoints, as its file gave it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportWaypoint {
    pub id: i64,
    /// 1-based, its place among its collection's waypoints.
    pub position: i64,
    pub name: Option<String>,
    pub description: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    /// The GPX `<sym>`, as the file names it (US-95).
    pub symbol: Option<String>,
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us89_a_collection_survives_the_round_trip_from_server_to_exporter() {
        let collection = ExportCollection {
            id: 3,
            name: "Rother: Jotunheimen".into(),
            description: "50 Touren".into(),
            created_at: "2026-10-01T10:00:00Z".into(),
            tracks: vec![ExportCollectionTrack {
                id: 12,
                name: "Besseggen".into(),
                activity_type: ActivityType::Hiking,
                distance_m: 14_000.0,
                ascent_m: 1_100.0,
                descent_m: 1_100.0,
                min_lat: 61.4,
                min_lon: 8.6,
                max_lat: 61.5,
                max_lon: 8.8,
            }],
            waypoints: vec![ExportWaypoint {
                id: 40,
                position: 1,
                name: None,
                description: Some("DNT".into()),
                lat: 61.48,
                lon: 8.62,
                ele: None,
                symbol: Some("Lodge".into()),
            }],
        };
        let json = serde_json::to_string(&collection).unwrap();
        assert_eq!(
            serde_json::from_str::<ExportCollection>(&json).unwrap(),
            collection
        );
    }
}
