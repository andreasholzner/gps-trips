//! What a tag summary lists and draws (US-107): per chosen tag, its recorded
//! and planned trips, the collections carrying it with all their tracks, and
//! the collection tracks it reaches on its own — each with its kind, its
//! line's color, and why it is not counted when it is not. Pure and
//! Dioxus-free, so the list and the map are decided where `cargo test`
//! reaches them; the script only draws what it is handed (ADR-0025).

use std::collections::HashMap;

use trip_archive_types::{
    ActivityType, StatsTrip, SummaryTrack, TagSummaries, TagTrips, TripKind, UncountedTrip,
};

use super::lines::tag_color;
use crate::activity_color;
use crate::kinds::Kind;
use crate::map_items::MapSource;

/// Why an entry adds nothing to the figures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Uncounted {
    Planned,
    FromCollection,
    WithoutTimestamps,
}

impl Uncounted {
    /// Said on hover and to a screen reader, after "not counted".
    pub fn reason(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::FromCollection => "from a collection",
            Self::WithoutTimestamps => "without timestamps",
        }
    }
}

/// One trip or collection track, as the list shows it and the map draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Which line style it is drawn in (US-90).
    pub kind: Kind,
    pub source: MapSource,
    pub id: i64,
    pub name: String,
    pub activity: ActivityType,
    /// The local date it starts on; `None` for a collection track, or a trip
    /// without timestamps.
    pub date: Option<String>,
    pub distance_m: f64,
    /// `None` for a trip the figures count.
    pub uncounted: Option<Uncounted>,
    /// Its line's color on the map.
    pub color: &'static str,
}

impl Entry {
    /// What tells it apart from every other entry: a trip and a collection
    /// track may share an id.
    pub fn key(&self) -> (MapSource, i64) {
        (self.source, self.id)
    }

    /// Its kind, named on hover and to a screen reader.
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            Kind::Recorded => "Recorded trip",
            Kind::Planned => "Planned trip",
            Kind::Collections => "Collection track",
        }
    }

    fn counted(trip: &StatsTrip) -> Self {
        Self {
            kind: Kind::Recorded,
            source: MapSource::Trips,
            id: trip.id,
            name: trip.name.clone(),
            activity: trip.activity_type,
            date: Some(trip.start_date.clone()),
            distance_m: trip.distance_m,
            uncounted: None,
            color: "",
        }
    }

    fn uncounted(trip: &UncountedTrip) -> Self {
        Self {
            kind: Kind::of_trip(trip.trip_kind),
            source: MapSource::Trips,
            id: trip.id,
            name: trip.name.clone(),
            activity: trip.activity_type,
            date: trip.start_date.clone(),
            distance_m: trip.distance_m,
            uncounted: Some(match trip.trip_kind {
                TripKind::Planned => Uncounted::Planned,
                TripKind::Recorded => Uncounted::WithoutTimestamps,
            }),
            color: "",
        }
    }

    fn track(track: &SummaryTrack) -> Self {
        Self {
            kind: Kind::Collections,
            source: MapSource::CollectionTracks,
            id: track.id,
            name: track.name.clone(),
            activity: track.activity_type,
            date: None,
            distance_m: track.distance_m,
            uncounted: Some(Uncounted::FromCollection),
            color: "",
        }
    }
}

/// One row of a tag's list: a trip or a track on its own, or a collection
/// with its tracks under it.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Entry(Entry),
    Collection {
        id: i64,
        name: String,
        /// The tag's color with several tags; with one, its tracks have
        /// colors of their own and it has none.
        color: Option<&'static str>,
        tracks: Vec<Entry>,
    },
}

/// One chosen tag's list.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub name: String,
    /// The tag's own color, with several tags chosen.
    pub color: Option<&'static str>,
    pub rows: Vec<Row>,
}

/// A group per chosen tag that holds anything, in the order chosen. What is
/// under several of them is listed under each, in the same color.
pub fn groups(data: &TagSummaries) -> Vec<Group> {
    let colors: HashMap<(MapSource, i64), &'static str> = drawn(data)
        .into_iter()
        .map(|entry| (entry.key(), entry.color))
        .collect();
    let paint = |entry: &mut Entry| entry.color = colors.get(&entry.key()).copied().unwrap_or("");
    let several = data.tags.len() > 1;
    data.tags
        .iter()
        .enumerate()
        .map(|(index, tag)| {
            let color = several.then(|| tag_color(index));
            let mut rows = rows(data, tag);
            for row in &mut rows {
                match row {
                    Row::Entry(entry) => paint(entry),
                    Row::Collection {
                        color: own, tracks, ..
                    } => {
                        *own = color;
                        tracks.iter_mut().for_each(paint);
                    }
                }
            }
            Group {
                name: tag.name.clone(),
                color,
                rows,
            }
        })
        .filter(|group| !group.rows.is_empty())
        .collect()
}

/// Every entry listed, once, in list order, in its line's color: with one
/// tag, a shade of its activity's color in that order (US-72); with several,
/// the color of the first chosen tag it is under. The map draws these.
pub fn drawn(data: &TagSummaries) -> Vec<Entry> {
    let mut entries: Vec<(usize, Entry)> = Vec::new();
    for (index, tag) in data.tags.iter().enumerate() {
        for entry in rows(data, tag).into_iter().flat_map(row_entries) {
            if !entries.iter().any(|(_, seen)| seen.key() == entry.key()) {
                entries.push((index, entry));
            }
        }
    }
    let colors = if data.tags.len() == 1 {
        activity_color::in_list_order(entries.iter().map(|(_, entry)| entry.activity))
    } else {
        entries.iter().map(|(index, _)| tag_color(*index)).collect()
    };
    entries
        .into_iter()
        .zip(colors)
        .map(|((_, entry), color)| Entry { color, ..entry })
        .collect()
}

fn row_entries(row: Row) -> Vec<Entry> {
    match row {
        Row::Entry(entry) => vec![entry],
        Row::Collection { tracks, .. } => tracks,
    }
}

/// `tag`'s rows, not yet colored: its trips oldest first, those without a
/// date last; then its collections by name; then the tracks it reaches on
/// their own, by collection name and file order — the orders the archive
/// answers in.
fn rows(data: &TagSummaries, tag: &TagTrips) -> Vec<Row> {
    let mut trips: Vec<Entry> = data
        .trips
        .iter()
        .filter(|trip| tag.trip_ids.contains(&trip.id))
        .map(Entry::counted)
        .chain(
            data.uncounted
                .iter()
                .filter(|trip| tag.uncounted_trip_ids.contains(&trip.id))
                .map(Entry::uncounted),
        )
        .collect();
    // Stable: the archive's order stands among trips of one date, and among
    // the undated, which it names alphabetically.
    trips.sort_by(|a, b| (a.date.is_none(), &a.date).cmp(&(b.date.is_none(), &b.date)));

    let collections = data
        .collections
        .iter()
        .filter(|collection| tag.collection_ids.contains(&collection.id))
        .map(|collection| Row::Collection {
            id: collection.id,
            name: collection.name.clone(),
            color: None,
            tracks: collection.tracks.iter().map(Entry::track).collect(),
        });
    let lone = data
        .collection_tracks
        .iter()
        .filter(|track| tag.collection_track_ids.contains(&track.id))
        .map(|track| Row::Entry(Entry::track(track)));

    trips
        .into_iter()
        .map(Row::Entry)
        .chain(collections)
        .chain(lone)
        .collect()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
