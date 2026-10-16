//! Whether a trip followed a plan or passed a waypoint (US-103, ADR-0030):
//! how much of the plan the trip covered and how much of the trip ran on
//! the plan, judged by the plan's line and by its own points. Pure: lines
//! come in, coverage goes out, and proposing a link is the caller's.

use crate::config::link_proposal as configured;
use crate::server::line_grid::{distance, Flat, Position, Projection, SegmentGrid};

/// The thresholds a pair is judged by.
#[derive(Debug, Clone, Copy)]
pub struct Rules {
    pub line_tolerance_m: f64,
    pub min_stretch_m: f64,
    pub plan_covered_share: f64,
    pub trip_on_plan_share: f64,
    pub min_covered_m: f64,
    pub point_tolerance_m: f64,
    pub points_reached_share: f64,
    pub min_point_run: usize,
    pub band_m: f64,
    pub waypoint_distance_m: f64,
}

impl Rules {
    /// The thresholds set in `config.rs`.
    pub const CONFIGURED: Self = Self {
        line_tolerance_m: configured::LINE_TOLERANCE_M,
        min_stretch_m: configured::MIN_STRETCH_M,
        plan_covered_share: configured::PLAN_COVERED_SHARE,
        trip_on_plan_share: configured::TRIP_ON_PLAN_SHARE,
        min_covered_m: configured::MIN_COVERED_M,
        point_tolerance_m: configured::POINT_TOLERANCE_M,
        points_reached_share: configured::POINTS_REACHED_SHARE,
        min_point_run: configured::MIN_POINT_RUN,
        band_m: configured::BAND_M,
        waypoint_distance_m: configured::WAYPOINT_DISTANCE_M,
    };
}

/// Which way of judging a plan found it followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Judged {
    /// By its line: the trip ran along it.
    Line,
    /// By its own points: the trip reached them, wherever it went between.
    Points,
}

/// How a trip covered a plan it followed, as shares from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coverage {
    /// Of the plan's line, the length the trip ran along.
    pub plan_covered: f64,
    /// Of the trip's line, the length that ran along the plan.
    pub trip_on_plan: f64,
    /// Of the plan's points, those the trip reached.
    pub points_reached: f64,
    pub judged: Judged,
}

/// How `trip` covered `plan`, if it followed it; `None` if it did not. The
/// direction either went in does not matter.
pub fn followed(trip: &[Position], plan: &[Position], rules: &Rules) -> Option<Coverage> {
    let &origin = plan.first()?;
    if trip.len() < 2 {
        return None;
    }
    let projection = Projection::around(origin);
    let trip: Vec<Flat> = trip.iter().map(|&p| projection.apply(p)).collect();
    let plan: Vec<Flat> = plan.iter().map(|&p| projection.apply(p)).collect();

    let near_trip = SegmentGrid::new(&trip, rules.line_tolerance_m);
    let near_plan = SegmentGrid::new(&plan, rules.line_tolerance_m);
    let plan_run = along(&plan, &near_trip, rules.min_stretch_m);
    let trip_run = along(&trip, &near_plan, rules.min_stretch_m);
    let reached = reached(&plan, &trip, rules.point_tolerance_m);
    let reached_count = reached.iter().filter(|&&r| r).count();
    let points_reached = reached_count as f64 / reached.len() as f64;

    let coverage = |judged| Coverage {
        plan_covered: plan_run.share(),
        trip_on_plan: trip_run.share(),
        points_reached,
        judged,
    };
    // The plan mostly covered — or the trip mostly on it, over enough of it.
    let by_line = plan_run.share() >= rules.plan_covered_share
        || (trip_run.share() >= rules.trip_on_plan_share
            && plan_run.covered_m >= rules.min_covered_m);
    if by_line {
        return Some(coverage(Judged::Line));
    }
    // The same two rules over the plan's own points: most of them reached —
    // or enough of them in a row, with the trip mostly within the band.
    let most_reached =
        reached_count >= rules.min_point_run && points_reached >= rules.points_reached_share;
    let by_points = most_reached || {
        let long_run = runs_of_reached(&plan, &reached)
            .any(|(points, length)| points >= rules.min_point_run && length >= rules.min_covered_m);
        long_run && {
            let near_band = SegmentGrid::new(&plan, rules.band_m);
            along(&trip, &near_band, rules.min_stretch_m).share() >= rules.trip_on_plan_share
        }
    };
    by_points.then(|| coverage(Judged::Points))
}

/// The indices of the `waypoints` that `line` passed.
pub fn waypoints_passed(line: &[Position], waypoints: &[Position], rules: &Rules) -> Vec<usize> {
    let Some(&origin) = line.first() else {
        return Vec::new();
    };
    let projection = Projection::around(origin);
    let line: Vec<Flat> = line.iter().map(|&p| projection.apply(p)).collect();
    let near_line = SegmentGrid::new(&line, rules.waypoint_distance_m);
    waypoints
        .iter()
        .enumerate()
        .filter(|&(_, &waypoint)| near_line.reaches(projection.apply(waypoint)))
        .map(|(index, _)| index)
        .collect()
}

/// Metres a line is measured in: each piece of it is near the other line or
/// not as its middle is.
const PIECE_M: f64 = 10.0;

/// How much of a line runs near another.
struct Run {
    covered_m: f64,
    length_m: f64,
}

impl Run {
    fn share(&self) -> f64 {
        if self.length_m > 0.0 {
            self.covered_m / self.length_m
        } else {
            0.0
        }
    }
}

/// How much of `line` runs within reach of `near`, counting only stretches
/// at least `min_stretch_m` long.
fn along(line: &[Flat], near: &SegmentGrid, min_stretch_m: f64) -> Run {
    let (mut covered_m, mut length_m, mut stretch_m) = (0.0, 0.0, 0.0);
    let mut close = |stretch_m: &mut f64| {
        if *stretch_m >= min_stretch_m {
            covered_m += *stretch_m;
        }
        *stretch_m = 0.0;
    };
    for pair in line.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let segment_m = distance(start, end);
        let pieces = (segment_m / PIECE_M).ceil().max(1.0);
        let piece_m = segment_m / pieces;
        for i in 0..pieces as usize {
            let t = (i as f64 + 0.5) / pieces;
            let middle = (
                start.0 + t * (end.0 - start.0),
                start.1 + t * (end.1 - start.1),
            );
            if near.reaches(middle) {
                stretch_m += piece_m;
            } else {
                close(&mut stretch_m);
            }
        }
        length_m += segment_m;
    }
    close(&mut stretch_m);
    Run {
        covered_m,
        length_m,
    }
}

/// Which of the plan's points lie within `tolerance_m` of the trip.
fn reached(plan: &[Flat], trip: &[Flat], tolerance_m: f64) -> Vec<bool> {
    let near_trip = SegmentGrid::new(trip, tolerance_m);
    plan.iter().map(|&point| near_trip.reaches(point)).collect()
}

/// Each run of reached points in a row: how many, and how far apart along
/// the plan its first and last are.
fn runs_of_reached<'a>(
    plan: &'a [Flat],
    reached: &'a [bool],
) -> impl Iterator<Item = (usize, f64)> + 'a {
    reached
        .chunk_by(|a, b| a == b)
        .scan(0, |start, chunk| {
            let from = *start;
            *start += chunk.len();
            Some((from, chunk))
        })
        .filter(|(_, chunk)| chunk[0])
        .map(move |(from, chunk)| {
            let length = plan[from..from + chunk.len()]
                .windows(2)
                .map(|pair| distance(pair[0], pair[1]))
                .sum();
            (chunk.len(), length)
        })
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
