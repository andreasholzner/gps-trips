use serde::{Deserialize, Serialize};

use crate::ActivityType;

/// A guide-book collection with its tracks (US-86): what its page shows.
/// Collections are records of their own, apart from trips (ADR-0028).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionDetail {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// `None` for a book with no activity focus.
    pub activity_type: Option<ActivityType>,
    pub waypoint_count: i64,
    /// In the order the file held them.
    pub tracks: Vec<CollectionTrackSummary>,
}

/// One of a collection's tracks, as its page lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionTrackSummary {
    pub id: i64,
    pub name: String,
    /// The activity the track goes by: its own, else its collection's, else
    /// `Unknown`.
    pub activity_type: ActivityType,
    pub distance_m: f64,
    pub ascent_m: f64,
    pub descent_m: f64,
}

/// A collection track's page (US-86): only distance, ascent and descent —
/// a guide book's timestamps are not when anyone went.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionTrackDetail {
    pub id: i64,
    pub collection_id: i64,
    pub collection_name: String,
    pub name: String,
    /// The activity the track goes by, as [`CollectionTrackSummary`]'s.
    pub activity_type: ActivityType,
    /// The track's own activity, overriding its collection's; `None` takes
    /// the collection's.
    pub own_activity: Option<ActivityType>,
    pub distance_m: f64,
    pub ascent_m: f64,
    pub descent_m: f64,
}

/// A waypoint from a guide book's file, as the file gave it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Waypoint {
    pub id: i64,
    /// The track it belongs to as well as to its collection — only ever one
    /// from a file with exactly one track.
    pub collection_track_id: Option<i64>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
}

/// What `POST /api/collections/preview` answers with: what the import
/// screen's guide-book form starts as, read from the file before anything is
/// created.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionPreview {
    pub suggested_name: String,
    pub suggested_description: String,
    pub track_count: usize,
    pub waypoint_count: usize,
}

/// What a created collection answers with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatedCollection {
    pub id: i64,
}

/// The `PATCH /api/collections/:id` body. An omitted field is left as it
/// is; a blank activity means none.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CollectionEdit {
    pub name: Option<String>,
    pub description: Option<String>,
    pub activity_type: Option<String>,
}

/// The `PATCH /api/collection-tracks/:id` body. An omitted field is left as
/// it is; a blank activity takes the collection's again.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CollectionTrackEdit {
    pub name: Option<String>,
    pub activity_type: Option<String>,
}
