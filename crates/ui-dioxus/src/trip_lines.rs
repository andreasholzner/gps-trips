//! The trip list's map zoomed in (US-73): past a zoom threshold it draws the
//! trips in view as their tracks, the way a share's overview map does,
//! instead of the heat marks (US-63).
//!
//! Which trips are in view, which lines to draw and in which color is
//! decided here, where `cargo test` reaches it; the region map's script only
//! reports where it is looking and draws what it is handed (ADR-0025).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use trip_archive_types::{ActivityType, TripSummary, TripTrack};

use crate::activity_color;
use crate::config::trip_map::LINES_FROM_ZOOM;
use crate::heat::HeatMarks;
use crate::interop::OverviewLine;
use crate::track;

/// Where the map is looking once it has settled: its zoom, and the visible
/// area as `[west, south, east, north]`. The script shifts the area back by
/// whole world widths, so it starts within the world, but a view across
/// the antimeridian still ends east of 180.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Viewport {
    pub zoom: f64,
    pub bounds: [f64; 4],
}

/// What the map draws in place of the heat marks: the lines, and where the
/// matching trips are, so "Fit to trips" still fits to all of them rather
/// than to the few in view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TripLines {
    pub lines: Vec<OverviewLine>,
    /// `[lat, lon]` of every matching trip's heat mark.
    pub fit: Vec<[f64; 2]>,
}

/// Whether the map is zoomed in far enough to draw lines.
pub fn shows_lines(viewport: &Viewport) -> bool {
    viewport.zoom >= LINES_FROM_ZOOM
}

/// The trips whose stored bounding box overlaps the visible area, in list
/// order. A trip without a box has no track to draw.
pub fn in_view<'a>(trips: &'a [TripSummary], viewport: &Viewport) -> Vec<&'a TripSummary> {
    let [west, south, east, north] = viewport.bounds;
    trips
        .iter()
        .filter(
            |trip| match (trip.min_lat, trip.min_lon, trip.max_lat, trip.max_lon) {
                (Some(min_lat), Some(min_lon), Some(max_lat), Some(max_lon)) => {
                    min_lat <= north
                        && max_lat >= south
                        && lon_ranges(west, east)
                            .iter()
                            .any(|&(from, to)| min_lon <= to && max_lon >= from)
                }
                _ => false,
            },
        )
        .collect()
}

/// The visible longitudes as ranges within `[-180, 180]`: one, or two for a
/// view across the antimeridian.
fn lon_ranges(west: f64, east: f64) -> Vec<(f64, f64)> {
    if east - west >= 360.0 {
        return vec![(-180.0, 180.0)];
    }
    let shift = if west < -180.0 { 360.0 } else { 0.0 };
    let (west, east) = (west + shift, east + shift);
    if east > 180.0 {
        vec![(west, 180.0), (-180.0, east - 360.0)]
    } else {
        vec![(west, east)]
    }
}

/// The tracks the map has read while the list is open, so a trip's track is
/// fetched at most once: panning back, or zooming out and in again, asks
/// the archive for nothing new. A track never changes once imported, and
/// the name and color of its line come from the list, which is read afresh.
#[derive(Debug, Default)]
pub struct TrackCache {
    /// Each trip asked for, with its track — or `None` where the archive
    /// sent none, so it is not asked for again either.
    known: HashMap<i64, Option<TripTrack>>,
}

impl TrackCache {
    /// Those of `ids` not asked for yet, in their order.
    pub fn missing(&self, ids: &[i64]) -> Vec<i64> {
        ids.iter()
            .copied()
            .filter(|id| !self.known.contains_key(id))
            .collect()
    }

    /// What the archive answered for `asked`; a trip it left out has no
    /// track to read.
    pub fn store(&mut self, asked: &[i64], answer: Vec<TripTrack>) {
        for id in asked {
            self.known.entry(*id).or_insert(None);
        }
        for track in answer {
            self.known.insert(track.id, Some(track));
        }
    }

    /// The tracks of `ids` that were read, in their order.
    pub fn tracks(&self, ids: &[i64]) -> Vec<&TripTrack> {
        ids.iter()
            .filter_map(|id| self.known.get(id)?.as_ref())
            .collect()
    }
}

/// Those of `missing` to ask the archive for: not the ones already asked
/// for, whose answer is still on its way. A view that settles while an
/// earlier one's tracks load waits for that answer rather than asking again.
pub fn to_request(missing: &[i64], pending: &HashSet<i64>) -> Vec<i64> {
    missing
        .iter()
        .copied()
        .filter(|id| !pending.contains(id))
        .collect()
}

/// One line per trip whose track was read, in list order, each in its shade
/// of its activity's color (US-72, US-75). The shades are taken over every
/// matching trip, not only those in view, so panning recolors nothing; and
/// `tracks` lacks those that could not be read, which shifts no other color.
pub fn lines(trips: &[TripSummary], tracks: &[&TripTrack], marks: &HeatMarks) -> TripLines {
    let colors = activity_color::in_list_order(trips.iter().map(|trip| trip.activity_type));
    let lines = trips
        .iter()
        .zip(colors)
        .filter_map(|(trip, color)| {
            let track = tracks.iter().find(|track| track.id == trip.id)?;
            Some(OverviewLine {
                id: trip.id,
                name: trip.name.clone(),
                color,
                points: track::lat_lon(&track.coordinates),
            })
        })
        .collect();
    TripLines {
        lines,
        fit: marks.marks.iter().map(|mark| mark.at).collect(),
    }
}

/// The activities of the trips that got a line, for the legend.
pub fn activities(trips: &[TripSummary], lines: &TripLines) -> Vec<ActivityType> {
    trips
        .iter()
        .filter(|trip| lines.lines.iter().any(|line| line.id == trip.id))
        .map(|trip| trip.activity_type)
        .collect()
}

#[cfg(test)]
mod tests;
