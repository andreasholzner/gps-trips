//! The trip list's map zoomed in (US-73): past a zoom threshold it draws the
//! trips in view as their tracks, the way a share's overview map does,
//! instead of the heat marks (US-63).
//!
//! Which trips are in view, which lines to draw and in which color is
//! decided here, where `cargo test` reaches it; the region map's script only
//! reports where it is looking and draws what it is handed (ADR-0025).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use trip_archive_types::{ActivityType, CollectionWaypoints, TripTrack};

use crate::activity_color;
use crate::config::trip_map::LINES_FROM_ZOOM;
use crate::heat::HeatMarks;
use crate::interop::MapWaypoint;
use crate::map_items::{MapItem, MapSource, Waypointed};
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
/// matching items are, so "Fit to items" still fits to all of them rather
/// than to the few in view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TripLines {
    pub lines: Vec<RegionLine>,
    /// The waypoints of the collections in view, shown with the lines
    /// (US-90).
    pub waypoints: Vec<MapWaypoint>,
    /// `[lat, lon]` of every matching item's bounding-box corners, and the
    /// list's waypoints.
    pub fit: Vec<[f64; 2]>,
}

/// One trip's or collection track's line on the region map.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegionLine {
    /// With `id`, what a click on it opens.
    pub source: MapSource,
    pub id: i64,
    pub name: String,
    /// Its shade of its activity's color (US-72, US-75).
    pub color: &'static str,
    /// Its kind's line style (US-90): `None` for a solid line.
    pub dash: Option<&'static str>,
    /// `[lat, lon]`, the order Leaflet takes.
    pub points: Vec<[f64; 2]>,
}

/// Whether the map is zoomed in far enough to draw lines.
pub fn shows_lines(viewport: &Viewport) -> bool {
    viewport.zoom >= LINES_FROM_ZOOM
}

/// The trips whose stored bounding box overlaps the visible area, in list
/// order. A trip without a box has no track to draw.
pub fn in_view<'a>(trips: &'a [MapItem], viewport: &Viewport) -> Vec<&'a MapItem> {
    trips
        .iter()
        .filter(|trip| {
            trip.bbox.is_some_and(|bbox| {
                overlaps(
                    [bbox.min_lon, bbox.min_lat, bbox.max_lon, bbox.max_lat],
                    viewport,
                )
            })
        })
        .collect()
}

/// The collections whose waypoints' rectangle overlaps the visible area
/// (US-90), in list order.
pub fn waypoints_in_view(waypointed: &[Waypointed], viewport: &Viewport) -> Vec<i64> {
    waypointed
        .iter()
        .filter(|w| overlaps(w.bounds, viewport))
        .map(|w| w.collection)
        .collect()
}

/// Whether `[west, south, east, north]` overlaps the visible area — edges
/// included, as the archive's region does (ADR-0011).
fn overlaps([min_lon, min_lat, max_lon, max_lat]: [f64; 4], viewport: &Viewport) -> bool {
    let [west, south, east, north] = viewport.bounds;
    min_lat <= north
        && max_lat >= south
        && lon_ranges(west, east)
            .iter()
            .any(|&(from, to)| min_lon <= to && max_lon >= from)
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

    /// Track `id`, if it was read.
    pub fn get(&self, id: i64) -> Option<&TripTrack> {
        self.known.get(&id)?.as_ref()
    }
}

/// The collections' waypoints the map has read while the list is open
/// (US-90), each collection's at most once, as [`TrackCache`] holds tracks.
/// An edit to a waypoint is seen once the list is opened again.
#[derive(Debug, Default)]
pub struct WaypointCache {
    /// Each collection read, with its waypoints — none for one the archive
    /// left out. One whose request failed is not here, so the next view
    /// that needs it asks again.
    known: HashMap<i64, Vec<MapWaypoint>>,
}

impl WaypointCache {
    /// Those of `collections` not asked for yet, in their order.
    pub fn missing(&self, collections: &[i64]) -> Vec<i64> {
        collections
            .iter()
            .copied()
            .filter(|id| !self.known.contains_key(id))
            .collect()
    }

    /// What the archive answered for `asked`; a collection it left out has
    /// no waypoints to show.
    pub fn store(&mut self, asked: &[i64], answer: Vec<CollectionWaypoints>) {
        for id in asked {
            self.known.entry(*id).or_default();
        }
        for collection in answer {
            self.known.insert(
                collection.id,
                collection.waypoints.iter().map(MapWaypoint::from).collect(),
            );
        }
    }

    /// The waypoints of `collections` that were read, in their order.
    pub fn waypoints(&self, collections: &[i64]) -> Vec<MapWaypoint> {
        collections
            .iter()
            .filter_map(|id| self.known.get(id))
            .flatten()
            .cloned()
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

/// One line per item whose track was read, in list order, each in its
/// shade of its activity's color (US-72, US-75) and its kind's line style
/// (US-90). The shades are taken over every matching item, not only those in
/// view, so panning recolors nothing; and `tracks` lacks those that could
/// not be read, which shifts no other color.
pub fn lines(
    items: &[MapItem],
    tracks: &[(MapSource, &TripTrack)],
    marks: &HeatMarks,
) -> TripLines {
    let colors = activity_color::in_list_order(items.iter().map(|item| item.activity_type));
    // Looked up by key, once each: the items are every one listed, of every
    // kind on, and the map redraws on each pan past the threshold.
    let tracks: HashMap<(MapSource, i64), &TripTrack> = tracks
        .iter()
        .map(|&(source, track)| ((source, track.id), track))
        .collect();
    let lines = items
        .iter()
        .zip(colors)
        .filter_map(|(item, color)| {
            let track = tracks.get(&item.key())?;
            Some(RegionLine {
                source: item.source(),
                id: item.id,
                name: item.name.clone(),
                color,
                dash: item.kind.dash(),
                points: track::lat_lon(&track.coordinates),
            })
        })
        .collect();
    TripLines {
        lines,
        waypoints: Vec::new(),
        fit: marks.fit_points(),
    }
}

impl TripLines {
    /// The same lines, with `waypoints` shown among them.
    pub fn with_waypoints(mut self, waypoints: Vec<MapWaypoint>) -> Self {
        self.waypoints = waypoints;
        self
    }
}

/// The activities of the items that got a line, for the legend.
pub fn activities(items: &[MapItem], lines: &TripLines) -> Vec<ActivityType> {
    let lined: HashSet<(MapSource, i64)> = lines
        .lines
        .iter()
        .map(|line| (line.source, line.id))
        .collect();
    items
        .iter()
        .filter(|item| lined.contains(&item.key()))
        .map(|item| item.activity_type)
        .collect()
}

#[cfg(test)]
mod tests;
