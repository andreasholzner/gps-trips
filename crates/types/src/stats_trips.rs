use serde::{Deserialize, Serialize};

use crate::{ActivityType, TripKind};

/// What `GET /api/stats/trips` answers (US-77): every recorded trip with
/// dates, reduced to what the statistics screen adds up. The figures
/// themselves are the SPA's to work out, so changing what the screen shows
/// asks nothing new of the archive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsTrips {
    pub trips: Vec<StatsTrip>,
    /// Recorded trips without timestamps: there is no year to count them
    /// in, so they are left out, and the screen says how many.
    pub undated: u32,
    /// Today's date, `YYYY-MM-DD` in UTC — the date the years are compared
    /// at. The archive's rather than the device's: the screen is rendered on
    /// targets with no clock of their own to read (ADR-0024).
    pub today: String,
}

/// One recorded trip as the statistics screen counts it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsTrip {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    /// The local dates the trip started and ended on, `YYYY-MM-DD`, in its
    /// own timezone, as `TripSummary::start_date` is.
    pub start_date: String,
    pub end_date: String,
    pub distance_m: f64,
    pub ascent_m: Option<f64>,
    /// What the tag summary adds up besides (US-78).
    pub descent_m: Option<f64>,
    /// Time spent moving (US-77); `None` until it has been worked out.
    pub moving_secs: Option<i64>,
    /// The distance covered meanwhile (US-80), on the same terms: the
    /// average speed in motion is the two's ratio.
    pub moving_distance_m: Option<f64>,
    /// What the climbing rate is worked out from (US-81): the height the
    /// climbs gain, added up, and the time spent moving on them. `None` for
    /// a track without times.
    pub climb_gain_m: Option<f64>,
    pub climb_secs: Option<i64>,
}

/// What `GET /api/stats/tags?tags=…` answers (US-78): the dated recorded
/// trips under any of the chosen tags, each once, and which tag holds which.
/// The figures are the SPA's to work out, as [`StatsTrips`]' are. Beside
/// them, what the tags hold that the figures do not count (US-107), each
/// once too.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TagSummaries {
    /// One per chosen tag, in the order asked.
    pub tags: Vec<TagTrips>,
    /// Oldest first.
    pub trips: Vec<StatsTrip>,
    /// Planned trips and recorded ones without timestamps: the dated ones
    /// oldest first, then the others by name.
    #[serde(default)]
    pub uncounted: Vec<UncountedTrip>,
    /// The collections carrying a tag, by name.
    #[serde(default)]
    pub collections: Vec<SummaryCollection>,
    /// The collection tracks carrying a tag their collection does not, by
    /// collection name, then in file order.
    #[serde(default)]
    pub collection_tracks: Vec<SummaryTrack>,
}

/// One chosen tag's part of [`TagSummaries`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TagTrips {
    /// Normalized, as the archive stores it.
    pub name: String,
    /// Its dated recorded trips, oldest first.
    pub trip_ids: Vec<i64>,
    /// Its recorded trips without timestamps, left out of `trip_ids`.
    pub undated: u32,
    /// Its trips in [`TagSummaries::uncounted`], in that order (US-107).
    #[serde(default)]
    pub uncounted_trip_ids: Vec<i64>,
    /// The collections carrying it, by name.
    #[serde(default)]
    pub collection_ids: Vec<i64>,
    /// The collection tracks carrying it whose collection does not.
    #[serde(default)]
    pub collection_track_ids: Vec<i64>,
}

/// A trip a tag summary lists without counting it (US-107).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UncountedTrip {
    pub id: i64,
    pub name: String,
    pub activity_type: ActivityType,
    pub trip_kind: TripKind,
    /// The local date it starts on, as [`StatsTrip::start_date`]; `None` for
    /// a track without timestamps.
    pub start_date: Option<String>,
    pub distance_m: f64,
}

/// A collection a tag summary lists (US-107), with all its tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryCollection {
    pub id: i64,
    pub name: String,
    /// In file order.
    pub tracks: Vec<SummaryTrack>,
}

/// A collection track a tag summary lists (US-107).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryTrack {
    pub id: i64,
    pub name: String,
    pub collection_name: String,
    /// Its own, else its collection's (US-86).
    pub activity_type: ActivityType,
    pub distance_m: f64,
}
