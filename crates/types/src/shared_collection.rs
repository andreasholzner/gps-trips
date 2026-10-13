//! US-94 — collections and their tracks as a share's recipient reads them.
//! Their own types rather than the owner's `CollectionDetail` and
//! `CollectionTrackDetail` (ADR-0015), so a field those grow later — a tag,
//! a kept file — reaches a stranger only by being added here.

use serde::{Deserialize, Serialize};

use crate::ActivityType;

/// One collection a share names, as its overview lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedCollection {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// `None` for a collection with no activity focus.
    pub activity_type: Option<ActivityType>,
    pub track_count: i64,
    pub waypoint_count: i64,
}

/// A shared collection's page: its details and its tracks in file order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedCollectionDetail {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub activity_type: Option<ActivityType>,
    pub waypoint_count: i64,
    pub tracks: Vec<SharedCollectionTrack>,
}

/// One collection track a share reaches, as a list or a map names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedCollectionTrack {
    pub id: i64,
    pub name: String,
    /// The activity the track goes by: its own, else its collection's, else
    /// `Unknown`.
    pub activity_type: ActivityType,
    pub distance_m: f64,
    pub ascent_m: f64,
    pub descent_m: f64,
}

/// A shared collection track's page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedCollectionTrackDetail {
    pub track: SharedCollectionTrack,
    /// The collection it is from — only when the share names that
    /// collection too, so a share of tracks never names theirs.
    pub collection: Option<SharedCollectionName>,
}

/// A shared collection as a track's page names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedCollectionName {
    pub id: i64,
    pub name: String,
}

/// A waypoint a share reaches, as the file gave it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedWaypoint {
    pub id: i64,
    pub name: Option<String>,
    pub description: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    /// The GPX `<sym>`, as the file names it.
    pub symbol: Option<String>,
}
