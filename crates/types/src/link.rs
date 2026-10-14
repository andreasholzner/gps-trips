//! US-99 — links from what the owner did to what they planned: a recorded
//! trip to a planned trip, and a recorded or planned trip to a collection
//! track or a waypoint. The request linking a selection, and what a page
//! reads about what it is linked to.

use serde::{Deserialize, Serialize};

use crate::ActivityType;

/// The `POST /api/links` body: what the trip list's selection holds (US-90).
/// Every pair in it that can be linked is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateLinks {
    /// Recorded and planned trips alike; each one's kind decides its pairs.
    pub trip_ids: Vec<i64>,
    pub collection_track_ids: Vec<i64>,
}

/// What `POST /api/links` answers with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedLinks {
    /// The pairs the selection holds, whether or not one was linked before.
    pub pairs: usize,
}

/// What a trip, a collection track or a waypoint is linked to, as its page
/// lists it. A list that cannot apply to it is empty: a recorded trip has no
/// recorded trips, a collection track no collection tracks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Links {
    /// Newest first.
    pub recorded_trips: Vec<LinkedTrip>,
    /// The rest by name.
    pub planned_trips: Vec<LinkedTrip>,
    pub collection_tracks: Vec<LinkedTrack>,
    pub waypoints: Vec<LinkedWaypoint>,
}

/// A recorded or planned trip at the other end of a link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkedTrip {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    /// The local date it started on, as `TripSummary::start_date`; `None`
    /// for a track with no timestamps.
    pub start_date: Option<String>,
}

/// A collection track at the other end of a link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkedTrack {
    pub id: i64,
    pub name: String,
    /// The activity the track goes by, its own or its collection's.
    pub activity_type: ActivityType,
    pub collection_id: i64,
    pub collection_name: String,
}

/// A waypoint at the other end of a link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkedWaypoint {
    pub id: i64,
    pub name: Option<String>,
    /// The GPX `<sym>`, for its icon (US-95).
    pub symbol: Option<String>,
    pub collection_id: i64,
    pub collection_name: String,
}
