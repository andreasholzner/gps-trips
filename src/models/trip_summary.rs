use serde::{Deserialize, Serialize};

use crate::models::ActivityType;

/// A lightweight trip row for the list view (US-6). Holds only the summary
/// fields shown in the list — never the track geometry (ADR-0003), so the list
/// query stays cheap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripSummary {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    pub start_time: Option<String>,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    pub duration_secs: Option<i64>,
}
