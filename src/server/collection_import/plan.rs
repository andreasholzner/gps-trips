//! The matched outcome as rows to store (`repo::apply_collection_import`):
//! each track's figures worked out from the file it now comes from, and each
//! waypoint's track named by its place among the outcome's tracks.

use std::collections::HashMap;

use crate::server::{
    collection_gpx::track_name,
    collection_match::{ItemKey, Membership, Origin},
    geojson::build_track_geojson,
    gpx::{compute_stats, TrackStats},
    repo::{NewCollectionTrack, NewWaypoint, PlannedTrack, PlannedWaypoint, TrackRef},
};

use super::Worked;

/// What a file's track is stored with.
pub(super) struct Derived {
    name: String,
    stats: TrackStats,
    geojson: String,
}

/// The figures of every file track the outcome takes. An unnamed one keeps
/// the name of the track it replaces; an added one is named after the
/// collection and its place in it, the tracks added going after the last of
/// the `held_last` it holds — so it takes no name one of them has.
pub(super) fn derive(
    worked: &Worked,
    collection_name: &str,
    held_last: i64,
) -> HashMap<ItemKey, Derived> {
    let mut place = held_last;
    worked
        .matching
        .outcome
        .tracks
        .iter()
        .filter_map(|final_track| {
            let (file, index) = final_track.source?;
            let track = &worked.parsed[file].tracks[index];
            if let Origin::File { .. } = final_track.origin {
                place += 1;
            }
            let name = match (&track.name, final_track.origin) {
                (Some(name), _) => name.clone(),
                (None, Origin::Held(id)) => worked
                    .held
                    .tracks
                    .iter()
                    .find(|held| held.id == id)
                    .map_or_else(String::new, |held| held.name.clone()),
                (None, Origin::File { .. }) => track_name(
                    None,
                    collection_name,
                    usize::try_from(place).unwrap_or_default(),
                ),
            };
            Some((
                (file, index),
                Derived {
                    name,
                    stats: compute_stats(&track.points),
                    geojson: build_track_geojson(&track.points),
                },
            ))
        })
        .collect()
}

pub(super) fn tracks<'a>(
    worked: &'a Worked,
    derived: &'a HashMap<ItemKey, Derived>,
) -> Vec<PlannedTrack<'a>> {
    worked
        .matching
        .outcome
        .tracks
        .iter()
        .map(|final_track| {
            let new = final_track.source.map(|(file, index)| {
                let derived = &derived[&(file, index)];
                NewCollectionTrack {
                    name: &derived.name,
                    stats: &derived.stats,
                    geojson: &derived.geojson,
                    gpx: &worked.parsed[file].tracks[index].gpx,
                }
            });
            match (final_track.origin, new) {
                (Origin::Held(id), None) => PlannedTrack::Keep(id),
                (Origin::Held(id), Some(new)) => PlannedTrack::Update(id, new),
                (Origin::File { .. }, Some(new)) => PlannedTrack::Insert(new),
                (Origin::File { .. }, None) => unreachable!("a file's track has its source"),
            }
        })
        .collect()
}

/// The waypoints the outcome changes or adds; those left as they are are not
/// listed.
pub(super) fn waypoints(worked: &Worked) -> Vec<PlannedWaypoint<'_>> {
    let outcome = &worked.matching.outcome;
    let place: HashMap<Origin, usize> = outcome
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| (track.origin, index))
        .collect();
    outcome
        .waypoints
        .iter()
        .filter_map(|final_waypoint| {
            let (file, index) = final_waypoint.source?;
            let wp = &worked.parsed[file].waypoints[index];
            // An unnamed waypoint keeps the name of the one it replaces.
            let held_name = match final_waypoint.origin {
                Origin::Held(id) => worked
                    .held
                    .waypoints
                    .iter()
                    .find(|held| held.id == id)
                    .and_then(|held| held.name.as_deref()),
                Origin::File { .. } => None,
            };
            let new = NewWaypoint {
                name: wp.name.as_deref().or(held_name),
                description: wp.description.as_deref(),
                lat: wp.lat,
                lon: wp.lon,
                ele: wp.ele,
                symbol: wp.symbol.as_deref(),
                track: None,
            };
            let track = match final_waypoint.track {
                Membership::Unchanged => TrackRef::Unchanged,
                Membership::Track(origin) => {
                    TrackRef::Set(origin.and_then(|origin| place.get(&origin).copied()))
                }
            };
            Some(match final_waypoint.origin {
                Origin::Held(id) => PlannedWaypoint::Update(id, new, track),
                Origin::File { .. } => PlannedWaypoint::Insert(new, track),
            })
        })
        .collect()
}
