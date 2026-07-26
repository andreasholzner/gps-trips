use serde::{Deserialize, Serialize};

use crate::models::KomootPrivacy;

/// A trip's `trip_komoot_link` row as the trip queries surface it (US-35).
/// Its presence *is* "this trip is Komoot-sourced" (ADR-0021), which is what
/// lets the detail page tell a trip that never came from Komoot (no privacy
/// control at all) from a linked one whose privacy simply isn't known yet
/// (`privacy: None`, until a sync reads it off Komoot's tour listing).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KomootLink {
    pub tour_id: String,
    pub privacy: Option<KomootPrivacy>,
}
