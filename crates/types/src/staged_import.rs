use serde::{Deserialize, Serialize};

/// What `POST /api/import/staged` answers with: everything the import screen
/// needs to fill in its second step, derived from the GPX the owner just
/// chose (US-12).
///
/// The archive cannot suggest a `YYYY-mm-dd` prefix before the name is
/// entered without reading the track first, so phase one parses the file and
/// parks the result; `staging_id` is the handle phase two confirms or
/// cancels. It is not a resource the API publishes — nothing reads it back —
/// only a continuation between two requests that expires on its own.
///
/// Deliberately a suggestion *object* rather than a bare name: an activity
/// type guessed from the track's shape, or the places it passes through,
/// would join it here without a second endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StagedImport {
    pub staging_id: i64,
    /// What the name field is prefilled with: `"2024-06-01 Oslo Hills Walk"`
    /// when the track carries a `<name>`, and the bare `"2024-06-01 "` prefix
    /// when it does not — the owner types the rest after the date, which is
    /// the whole point of US-12. Empty when the track has neither.
    pub suggested_name: String,
    /// The track's start date as `YYYY-MM-DD`, or `None` for a GPX with no
    /// timestamps at all.
    pub start_date: Option<String>,
    /// The GPX track's own `<name>`, shown as-is so the owner can tell which
    /// file they picked.
    pub gpx_name: Option<String>,
    /// The timezone guessed from the track's start coordinate (US-4,
    /// ADR-0019), prefilled into the override field.
    pub timezone: String,
    pub distance_m: f64,
    pub ascent_m: f64,
    pub duration_secs: Option<i64>,
}

/// The `POST /api/import/staged/:id/confirm` body: the owner's answers to
/// what the suggestion proposed.
///
/// Every field is optional and carries exactly the meaning the single-step
/// import form's equivalent text field always had — an absent or blank one
/// falls back the same way, so the two entry points cannot drift on what
/// "left it alone" means. The wire values, not the labels (ADR-0018).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfirmImport {
    /// Blank or absent falls back to the GPX track's name, then to a
    /// date-prefixed default.
    pub name: Option<String>,
    /// Blank or absent means `unknown` (US-11).
    pub activity_type: Option<String>,
    /// Blank or absent means `recorded` (US-31).
    pub kind: Option<String>,
    /// Blank or absent means the timezone staging guessed from the track.
    pub timezone: Option<String>,
}

/// What a confirmed import answers with: the trip that now exists.
///
/// A JSON body rather than the `303` the one-shot `POST /api/import` still
/// answers with, because this one is read by a browser `fetch`, which cannot
/// decline to follow a redirect — the caller would be reading the status of
/// whatever it landed on as the import's (the same reason
/// `POST /api/trips/:id/photos` answers `204`, US-42).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedTrip {
    pub id: i64,
}
