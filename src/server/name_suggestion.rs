//! The place-based name suggestion (US-74): what a trip's name says behind
//! its date, read from the places its track passes.
//!
//! A trip that ends somewhere other than where it started reads `A - B`,
//! its start and its end; one that ends near its start reads `A: D1 - D2`,
//! its start and the main places it went to. An end is named after the
//! most important place within reach of it — or after the hut or campsite
//! it stopped at; the main places are the weightiest the track passes close
//! to, in the order it passes them. Every reach and weight is in
//! `config::name_suggestion`.
//!
//! Pure, so it is tested directly over synthetic places (ADR-0012); the
//! places come from `places::PlaceDb`.

mod geometry;
mod ranking;
#[cfg(test)]
mod tests;

use geo::{Coord, Rect};

use crate::config::name_suggestion::{
    end_reach_m, main_reach_m, DOMINANCE_RATIO, MAX_MAIN_PLACES, NAMESAKE_REACH_M, ROUND_TRIP_M,
    STOP_REACH_M, TURNING_POINT_BONUS,
};
use crate::server::gpx::TrackPoint;
use crate::server::places::Place;

use geometry::{bounds, distance, Plane, Track};
use ranking::{dedupe, Located};

/// The farthest any place reaches, from an end or from the track: how far
/// around them places are looked up.
pub const LOOKUP_REACH_M: f64 = NAMESAKE_REACH_M;

/// The part of the name behind the date for the track through `points`,
/// from `places` around it; `None` when no place names it.
pub fn place_part(points: &[TrackPoint], places: Vec<Place>) -> Option<String> {
    if points.is_empty() {
        return None;
    }
    let coords: Vec<Coord> = points
        .iter()
        .map(|p| Coord { x: p.lon, y: p.lat })
        .collect();
    let plane = Plane::around(&coords);
    let track = Track::new(
        coords.iter().map(|c| plane.project(*c)).collect(),
        points.iter().map(|p| p.ele).collect(),
    );
    let places = dedupe(
        places
            .into_iter()
            .map(|p| Located::new(p, &plane))
            .collect(),
    );

    let start = end_name(track.start(), &places);
    let mut round = distance(track.start(), track.end()) <= ROUND_TRIP_M;
    let end = if round {
        None
    } else {
        end_name(track.end(), &places)
    };
    if start.is_some() && start == end {
        round = true;
    }
    if let (false, Some(start), Some(end)) = (round, &start, &end) {
        return Some(format!("{start} - {end}"));
    }

    let ends: Vec<&str> = start.iter().chain(&end).map(String::as_str).collect();
    let mains = main_places(&track, &places, round, &ends);
    let parts = if round {
        if let Some(start) = &start {
            return Some(if mains.is_empty() {
                start.clone()
            } else {
                format!("{start}: {}", mains.join(" - "))
            });
        }
        mains
    } else {
        let mut parts = start.into_iter().collect::<Vec<_>>();
        parts.extend(mains);
        parts.extend(end);
        parts
    };
    (!parts.is_empty()).then(|| parts.join(" - "))
}

/// The boxes (longitude/latitude) to look places up in for the track
/// through `coords`: around each stretch of it, wide enough for the
/// farthest reach. Stretch by stretch, so a long trip does not fetch every
/// place in the rectangle it spans.
pub fn lookup_boxes(coords: &[Coord]) -> Vec<Rect> {
    const STRETCH_POINTS: usize = 200;
    let Some(first) = coords.first() else {
        return Vec::new();
    };
    let margin = Coord {
        x: LOOKUP_REACH_M / (111_320.0 * first.y.to_radians().cos().max(0.1)),
        y: LOOKUP_REACH_M / 110_574.0,
    };
    (0..coords.len())
        .step_by(STRETCH_POINTS)
        .map(|from| {
            // Each stretch ends where the next one starts, so no segment
            // falls between two boxes.
            let to = (from + STRETCH_POINTS + 1).min(coords.len());
            let (min, max) = bounds(&coords[from..to]);
            Rect::new(min - margin, max + margin)
        })
        .collect()
}

/// What names the end at `at`: the hut or campsite the trip stopped at,
/// under the name of the place it is named after if there is one, or else
/// the weightiest place within its kind's reach — the nearer of two that
/// weigh the same.
fn end_name(at: Coord, places: &[Located]) -> Option<String> {
    let stop = places
        .iter()
        .filter(|p| p.place.kind.is_stop())
        .map(|p| (p, p.flat.distance_to(at)))
        .filter(|(_, d)| *d <= STOP_REACH_M)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((stop, _)) = stop {
        return Some(namesake(stop, places).unwrap_or_else(|| stop.place.name.clone()));
    }
    places
        .iter()
        .filter_map(|p| {
            let reach = end_reach_m(p.place.kind)?;
            let d = p.flat.distance_to(at);
            (d <= reach).then_some((p, d))
        })
        .max_by(|(a, da), (b, db)| a.weight().total_cmp(&b.weight()).then(db.total_cmp(da)))
        .map(|(p, _)| p.place.name.clone())
}

/// The place `stop` is named after: one nearby whose name its name starts
/// with, as a word (`Fjordbotn Camping` after `Fjordbotn`) — the longest
/// such name, then the nearest.
fn namesake(stop: &Located, places: &[Located]) -> Option<String> {
    let name = stop.place.name.to_lowercase();
    places
        .iter()
        .filter(|p| !p.place.kind.is_stop() && !p.place.is_height_only())
        .filter(|p| {
            let candidate = p.place.name.to_lowercase();
            name.strip_prefix(&candidate)
                .is_some_and(|rest| rest.starts_with(' '))
        })
        .map(|p| (p, p.flat.distance_to_shape(&stop.flat)))
        .filter(|(_, d)| *d <= NAMESAKE_REACH_M)
        .max_by(|(a, da), (b, db)| {
            a.place
                .name
                .len()
                .cmp(&b.place.name.len())
                .then(db.total_cmp(da))
        })
        .map(|(p, _)| p.place.name.clone())
}

/// The main places of the trip, in the order the track passes them: the
/// weightiest it passes close to, one name once, none of the `ends`. On a
/// round trip, a place weighs more the nearer it lies to the turning point,
/// and one [`DOMINANCE_RATIO`] times weightier than another leaves that one
/// out — one far weightier than all the rest stands alone.
fn main_places(track: &Track, places: &[Located], round: bool, ends: &[&str]) -> Vec<String> {
    let turning = track.points[track.turning_point()];
    let span = distance(track.start(), turning).max(1.0);
    let mut passed: Vec<(&Located, usize, f64)> = places
        .iter()
        .filter(|p| !ends.contains(&p.place.name.as_str()))
        .filter_map(|p| {
            let passing = track.passing(&p.flat, main_reach_m(p.place.kind)?)?;
            let mut weight = p.weight();
            if round {
                let closeness = 1.0 - distance(track.points[passing.index], turning) / span;
                weight *= 1.0 + TURNING_POINT_BONUS * closeness.max(0.0).powi(3);
            }
            Some((p, passing.index, weight))
        })
        .collect();
    passed.sort_by(|a, b| b.2.total_cmp(&a.2));

    let mut chosen: Vec<(&Located, usize, f64)> = Vec::new();
    for candidate in passed {
        if !chosen
            .iter()
            .any(|c| c.0.place.name == candidate.0.place.name)
        {
            chosen.push(candidate);
        }
    }
    if round {
        let least = chosen.first().map_or(0.0, |top| top.2 / DOMINANCE_RATIO);
        chosen.retain(|c| c.2 > least);
    }
    chosen.truncate(MAX_MAIN_PLACES);
    chosen.sort_by_key(|c| c.1);
    chosen.into_iter().map(|c| c.0.place.name.clone()).collect()
}
