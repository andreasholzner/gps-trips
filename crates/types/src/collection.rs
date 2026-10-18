use serde::{Deserialize, Serialize};

use crate::ActivityType;

/// A collection with its tracks (US-86): what its page shows.
/// Collections are records of their own, apart from trips (ADR-0028).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionDetail {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// `None` for a collection with no activity focus.
    pub activity_type: Option<ActivityType>,
    pub waypoint_count: i64,
    /// In the order the file held them.
    pub tracks: Vec<CollectionTrackSummary>,
    /// The waypoints a recorded trip is linked to (US-99), in file order.
    pub waypoints_with_recorded_trip: Vec<i64>,
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
    /// Whether a recorded trip is linked to it (US-99).
    pub has_recorded_trip: bool,
}

/// A collection as the trip list's Collections tab lists it (US-87), with
/// what its map needs: where each of its tracks lies, and where its
/// waypoints do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionSummary {
    pub id: i64,
    pub name: String,
    /// `None` for a collection with no activity focus.
    pub activity_type: Option<ActivityType>,
    pub waypoint_count: i64,
    /// How many tracks it holds in all, whatever the filters (US-90).
    pub track_count: i64,
    /// How many of them a recorded trip is linked to, whatever the filters
    /// (US-99).
    pub tracks_with_recorded_trip: i64,
    /// The tracks the filters match — all of them with none set — in the
    /// order the file held them.
    pub tracks: Vec<CollectionTrackMark>,
    /// The rectangle around every waypoint, `[west, south, east, north]`;
    /// `None` for a collection without waypoints.
    pub waypoint_bounds: Option<[f64; 4]>,
    /// The names of its own tags, alphabetically (US-113) — not its
    /// tracks' (US-93).
    pub tags: Vec<String>,
}

/// One collection's waypoints, as `GET /api/collection-waypoints?ids=…`
/// answers for the trip list's map to show with the lines (US-90).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionWaypoints {
    /// The collection's id.
    pub id: i64,
    /// In file order.
    pub waypoints: Vec<Waypoint>,
}

/// What `GET /api/collections/count` answers with: how many collections
/// there are, for the Collections tab's count line (US-87).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CollectionCount {
    pub count: i64,
}

/// One of a listed collection's tracks: enough to mark it on the map, name
/// its line and color it, and list it under its collection (US-90).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionTrackMark {
    pub id: i64,
    pub name: String,
    /// The activity the track goes by, as [`CollectionTrackSummary`]'s.
    pub activity_type: ActivityType,
    /// What its row in the trip list shows (US-90).
    pub distance_m: f64,
    pub ascent_m: f64,
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_lat: f64,
    pub max_lon: f64,
    /// Whether a recorded trip is linked to it (US-99).
    pub has_recorded_trip: bool,
    /// The names of its own tags, alphabetically (US-113).
    pub tags: Vec<String>,
}

/// A collection track's page (US-86): only distance, ascent and descent —
/// a collection's timestamps are not when anyone went.
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
    /// The collection's activity, which a track without its own goes by;
    /// `None` when the collection has none.
    pub collection_activity: Option<ActivityType>,
    pub distance_m: f64,
    pub ascent_m: f64,
    pub descent_m: f64,
}

/// A waypoint from a collection's file, as the file gave it.
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
    /// The GPX `<sym>`, as the file names it — `None` also for a waypoint
    /// imported before symbols were kept (US-95).
    pub symbol: Option<String>,
}

/// What `POST /api/collections/preview` answers with: what the import
/// screen's collection form starts as, read from the file before anything is
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

/// What `GET /api/waypoints/:id` answers with: the waypoint and where it
/// belongs, for its own page (US-98).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaypointDetail {
    pub waypoint: Waypoint,
    pub collection_id: i64,
    pub collection_name: String,
    /// The track it belongs to as well, whose line its page draws.
    pub track: Option<WaypointTrack>,
}

/// The track a waypoint belongs to, as its page needs it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaypointTrack {
    pub id: i64,
    pub name: String,
    /// The activity it goes by, its own or its collection's — its line's
    /// color.
    pub activity_type: ActivityType,
}

/// The `PATCH /api/waypoints/:id` body (US-98). An omitted field is left as
/// it is; a blank description, symbol or elevation takes it away. The
/// elevation is the text the form holds, in metres. A new position gives
/// both `lat` and `lon`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WaypointEdit {
    pub name: Option<String>,
    pub description: Option<String>,
    pub symbol: Option<String>,
    pub ele: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}
