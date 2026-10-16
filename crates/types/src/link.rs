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

/// A link the archive proposed from the tracks' geometry and the owner has
/// not confirmed yet (US-103, ADR-0030), with what it was proposed on, as
/// shares from 0 to 1: a plan followed along its line has the first two, one
/// judged by its own points the last, a waypoint passed none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    /// Of the plan's line, the length the trip ran along.
    pub plan_covered: Option<f64>,
    /// Of the trip's line, the length that ran along the plan.
    pub trip_on_plan: Option<f64>,
    /// Of the plan's own points, those the trip reached.
    pub points_reached: Option<f64>,
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
    /// `None` for a link; a proposal until the owner confirms it.
    #[serde(default)]
    pub proposal: Option<Proposal>,
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
    /// `None` for a link; a proposal until the owner confirms it.
    #[serde(default)]
    pub proposal: Option<Proposal>,
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
    /// `None` for a link; a proposal until the owner confirms it.
    #[serde(default)]
    pub proposal: Option<Proposal>,
}

/// What a shared trip is linked to, as a share's recipient reads it: only
/// the trips the share reaches too. The recipient's own types, so nothing
/// the owner's grow later reaches a stranger by itself (ADR-0015).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SharedLinks {
    /// Newest first.
    pub recorded_trips: Vec<SharedLinkedTrip>,
    /// By name.
    pub planned_trips: Vec<SharedLinkedTrip>,
}

/// A shared trip at the other end of a link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedLinkedTrip {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    pub start_date: Option<String>,
}

/// Whether a recorded trip is linked to it — the trip list's filter on
/// planned trips and collection tracks, and a collection page's on its
/// tracks and waypoints. A closed set on the wire, so an enum (ADR-0018).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordedTripFilter {
    /// Only what has a recorded trip linked to it.
    With,
    /// Only what has none.
    Without,
}

impl RecordedTripFilter {
    pub const ALL: [RecordedTripFilter; 2] = [Self::With, Self::Without];

    /// The `recorded_trip` query parameter's value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::With => "with",
            Self::Without => "without",
        }
    }
}

impl std::fmt::Display for RecordedTripFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for RecordedTripFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|filter| filter.as_str() == s)
            .ok_or_else(|| {
                format!("unknown recorded_trip filter: {s:?} (expected with or without)")
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recorded_trip_filter_round_trips_through_its_query_value() {
        for filter in RecordedTripFilter::ALL {
            assert_eq!(filter.to_string().parse::<RecordedTripFilter>(), Ok(filter));
        }
        assert!("maybe".parse::<RecordedTripFilter>().is_err());
    }
}
