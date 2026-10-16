//! US-103 — looking for links to propose (ADR-0030): what enters the archive,
//! or what the owner starts a check from, against everything it can be
//! linked to that lies near it. Runs within the request that asks for it —
//! there is no background job.

use sqlx::SqlitePool;

use crate::models::{Proposal, TripKind};
use crate::server::link_match::{followed, waypoints_passed, Coverage, Judged, Rules};
use crate::server::repo::{self, Extent, Line, Place, ProposalPair};

const RULES: Rules = Rules::CONFIGURED;

/// How far beyond a line's extent anything it can be proposed with may lie.
fn reach_m() -> f64 {
    RULES
        .line_tolerance_m
        .max(RULES.point_tolerance_m)
        .max(RULES.waypoint_distance_m)
}

/// Look for links for a trip, as if it had just entered the archive. How
/// many were proposed; `None` if it does not exist.
pub async fn check_trip(pool: &SqlitePool, id: i64) -> Result<Option<usize>, sqlx::Error> {
    let Some((kind, trip)) = repo::trip_line(pool, id).await? else {
        return Ok(None);
    };
    let Some(extent) = Extent::around(trip.line.iter().copied()) else {
        return Ok(Some(0));
    };
    let near = extent.widened(reach_m());
    let mut proposals = Vec::new();
    match kind {
        TripKind::Recorded => {
            for plan in repo::trips_near(pool, TripKind::Planned, near).await? {
                if let Some(coverage) = followed(&trip.line, &plan.line, &RULES) {
                    let pair = ProposalPair::Plan {
                        recorded: id,
                        planned: plan.id,
                    };
                    proposals.push((pair, proposal(coverage)));
                }
            }
        }
        TripKind::Planned => {
            for recorded in repo::trips_near(pool, TripKind::Recorded, near).await? {
                if let Some(coverage) = followed(&recorded.line, &trip.line, &RULES) {
                    let pair = ProposalPair::Plan {
                        recorded: recorded.id,
                        planned: id,
                    };
                    proposals.push((pair, proposal(coverage)));
                }
            }
        }
    }
    for track in repo::tracks_near(pool, near).await? {
        if let Some(coverage) = followed(&trip.line, &track.line, &RULES) {
            let pair = ProposalPair::Track {
                trip: id,
                track: track.id,
            };
            proposals.push((pair, proposal(coverage)));
        }
    }
    let waypoints = repo::waypoints_near(pool, near).await?;
    proposals.extend(passed(&trip, &waypoints));
    Ok(Some(repo::propose(pool, &proposals).await?))
}

/// Look for links for a collection's tracks and waypoints, as if they had
/// just entered the archive. How many were proposed; `None` if it does not
/// exist.
pub async fn check_collection(pool: &SqlitePool, id: i64) -> Result<Option<usize>, sqlx::Error> {
    let Some((tracks, waypoints)) = repo::collection_item_ids(pool, id).await? else {
        return Ok(None);
    };
    Ok(Some(
        check_collection_items(pool, &tracks, &waypoints).await?,
    ))
}

/// Look for links for these collection tracks and waypoints. How many were
/// proposed.
pub async fn check_collection_items(
    pool: &SqlitePool,
    track_ids: &[i64],
    waypoint_ids: &[i64],
) -> Result<usize, sqlx::Error> {
    let (tracks, waypoints) = repo::collection_items(pool, track_ids, waypoint_ids).await?;
    let mut proposals = Vec::new();
    for track in &tracks {
        let Some(extent) = Extent::around(track.line.iter().copied()) else {
            continue;
        };
        for trip in trips_near(pool, extent.widened(reach_m())).await? {
            if let Some(coverage) = followed(&trip.line, &track.line, &RULES) {
                let pair = ProposalPair::Track {
                    trip: trip.id,
                    track: track.id,
                };
                proposals.push((pair, proposal(coverage)));
            }
        }
    }
    for waypoint in &waypoints {
        let near = Extent::around([waypoint.at])
            .expect("a position")
            .widened(RULES.waypoint_distance_m);
        for trip in trips_near(pool, near).await? {
            proposals.extend(passed(&trip, std::slice::from_ref(waypoint)));
        }
    }
    repo::propose(pool, &proposals).await
}

/// Look for links for what entered the archive, within the request that
/// brought it in. A failed check is logged and leaves what entered as it
/// is: the owner can start the check again from its page.
pub async fn after_trip_entered(pool: &SqlitePool, id: i64) {
    if let Err(err) = check_trip(pool, id).await {
        tracing::warn!(trip = id, "looking for links to propose failed: {err}");
    }
}

/// As [`after_trip_entered`], for collection tracks and waypoints, new or
/// replaced by a file: what was proposed for a replaced one before is
/// forgotten, and it is checked as if new.
pub async fn after_items_entered(pool: &SqlitePool, track_ids: &[i64], waypoint_ids: &[i64]) {
    let checked = async {
        repo::forget_proposals_of(pool, track_ids, waypoint_ids).await?;
        check_collection_items(pool, track_ids, waypoint_ids).await
    };
    if let Err(err) = checked.await {
        tracing::warn!("looking for links to propose failed: {err}");
    }
}

/// Recorded and planned trips alike.
async fn trips_near(pool: &SqlitePool, extent: Extent) -> Result<Vec<Line>, sqlx::Error> {
    let mut trips = repo::trips_near(pool, TripKind::Recorded, extent).await?;
    trips.extend(repo::trips_near(pool, TripKind::Planned, extent).await?);
    Ok(trips)
}

/// A proposal for each of `waypoints` the trip passed.
fn passed(trip: &Line, waypoints: &[Place]) -> Vec<(ProposalPair, Proposal)> {
    let positions: Vec<_> = waypoints.iter().map(|place| place.at).collect();
    waypoints_passed(&trip.line, &positions, &RULES)
        .into_iter()
        .map(|index| {
            let pair = ProposalPair::Waypoint {
                trip: trip.id,
                waypoint: waypoints[index].id,
            };
            (pair, Proposal::default())
        })
        .collect()
}

/// What a proposal is read by: the line's two shares, or the points' share.
fn proposal(coverage: Coverage) -> Proposal {
    match coverage.judged {
        Judged::Line => Proposal {
            plan_covered: Some(coverage.plan_covered),
            trip_on_plan: Some(coverage.trip_on_plan),
            points_reached: None,
        },
        Judged::Points => Proposal {
            points_reached: Some(coverage.points_reached),
            ..Proposal::default()
        },
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
