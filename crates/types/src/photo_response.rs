use serde::{Deserialize, Serialize};

use crate::{LocationSource, Photo};

/// The JSON shape returned by `GET /api/trips/:id/photos` (ADR-0008).
///
/// Wraps the stored [`Photo`] record and adds the public `url`/`thumbnail_url`
/// the client uses to fetch the image bytes. Those come from the `BlobStore`
/// (ADR-0007), which lives on the server — so they arrive here as plain
/// strings ([ADR-0015](../../../docs/adr/0015-db-model-response-type-separation.md)'s
/// 2026-08-28 amendment: a response type carries no server dependency, which
/// is what lets it live beside the stored records and serve both sides of the
/// API). `lat`/`lon`/`location_source` (US-3/US-4) are derived once at import
/// and persisted, so — unlike `url` — they travel straight from `photo` with
/// no extra constructor argument. `thumbnail_url` (US-5) is always populated —
/// it falls back to the full-size `url` when a photo has no thumbnail
/// (generation failed, or the photo predates US-5) — so the client never has
/// to branch on its absence.
///
/// `Deserialize` and `PartialEq` because the SPA reads this shape and hands it
/// to components as props (ADR-0024).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhotoResponse {
    pub id: i64,
    pub trip_id: i64,
    pub original_name: String,
    pub content_type: Option<String>,
    pub byte_len: i64,
    pub created_at: String,
    /// When the photo was taken, RFC-3339 UTC (US-62).
    pub taken_at: Option<String>,
    /// The UTC offset, in seconds, in force where and when the photo was
    /// taken — from its own position where it has one, the trip's zone where
    /// it does not (US-62). `None` when neither resolves: the caption then
    /// says its time is UTC rather than invent a zone.
    pub taken_offset_secs: Option<i32>,
    pub url: String,
    pub thumbnail_url: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub location_source: LocationSource,
    /// Whether the photo still needs placing by hand (US-106): left
    /// unplaced, or parked by Komoot at its track's start or end — the
    /// "Unplaced photos" filter's own rule (US-101), decided by the server
    /// because the gallery has no track to ask. Always `false` for a share's
    /// recipient, who cannot place a photo.
    pub needs_placing: bool,
}

impl PhotoResponse {
    /// Project a stored record into its wire shape, with the serving URLs the
    /// blob store computed, and the offset the photo was taken
    /// at — which needs the server's timezone lookups (US-62) — and whether
    /// it needs placing, which needs its track (US-106). Field by field on purpose: a new field on either
    /// type is a compile error here until it is decided which side it belongs
    /// to (ADR-0015).
    pub fn from_photo(
        photo: Photo,
        url: String,
        thumbnail_url: String,
        taken_offset_secs: Option<i32>,
        needs_placing: bool,
    ) -> Self {
        Self {
            id: photo.id,
            trip_id: photo.trip_id,
            original_name: photo.original_name,
            content_type: photo.content_type,
            byte_len: photo.byte_len,
            created_at: photo.created_at,
            taken_at: photo.taken_at,
            taken_offset_secs,
            url,
            thumbnail_url,
            lat: photo.lat,
            lon: photo.lon,
            location_source: photo.location_source,
            needs_placing,
        }
    }
}
