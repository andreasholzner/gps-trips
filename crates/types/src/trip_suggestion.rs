use serde::{Deserialize, Serialize};

/// What `GET /api/trips/:id/suggestion` answers with: what the edit form
/// offers next to its fields for the trip as it is stored (US-74). Only
/// offered — nothing changes without the owner's click.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TripSuggestion {
    /// The trip's date, then the places its track passes, as the import
    /// screen would suggest it: `"2019-09-07 Rysstad - Kilefjorden"`.
    pub name: String,
}
