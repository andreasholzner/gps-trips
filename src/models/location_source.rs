use serde::{Deserialize, Serialize};

/// How a photo's map coordinates were determined (ADR-0018: a closed,
/// application-defined set of values modeled as an enum rather than a bare
/// string). Stored as `TEXT` in SQLite (`#[derive(sqlx::Type)]` maps each
/// variant to/from its lowercase name) and serialized the same way in JSON
/// responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum LocationSource {
    /// No GPS could be determined (no EXIF GPS, unparseable, or out of range).
    None,
    /// Read from the photo's EXIF GPS tags (US-3).
    Exif,
    /// Estimated from nearby track points by timestamp (US-4).
    Interpolated,
    /// Supplied directly by an external source (e.g. Komoot's own photo GPS
    /// record, US-22) rather than derived from this app's own EXIF/
    /// interpolation pipeline.
    Provided,
}
