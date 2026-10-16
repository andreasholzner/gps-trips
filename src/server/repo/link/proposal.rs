//! US-103 — links the archive proposes from the tracks' geometry (ADR-0030):
//! a link marked as proposed, until the owner confirms it (the mark cleared)
//! or dismisses it (the row deleted). What to judge is read in `candidates`,
//! which pairs were followed is `link_match`'s.

use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqlitePool, Transaction};

use super::super::collection::TRACK_ACTIVITY;
use super::super::trip::local_start_date;
use super::by_name;
use crate::models::{
    LinkOther, LinkPair, LinkedTrack, LinkedTrip, LinkedWaypoint, Proposal, ProposedEnd,
    ProposedLink, TripKind,
};
use crate::server::db;

/// A pair a proposal joins, each end in its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalPair {
    Plan { recorded: i64, planned: i64 },
    Track { trip: i64, track: i64 },
    Waypoint { trip: i64, waypoint: i64 },
}

/// Propose each pair with what it was proposed on, in one transaction. A
/// pair already linked or proposed stays as it is. How many were new.
pub async fn propose(
    pool: &SqlitePool,
    proposals: &[(ProposalPair, Proposal)],
) -> Result<usize, sqlx::Error> {
    let mut tx = db::begin_write(pool).await?;
    let mut new = 0;
    for (pair, proposal) in proposals {
        let (table, columns, ends) = match *pair {
            ProposalPair::Plan { recorded, planned } => (
                "trip_trip_link",
                "recorded_trip_id, planned_trip_id",
                (recorded, planned),
            ),
            ProposalPair::Track { trip, track } => (
                "trip_track_link",
                "trip_id, collection_track_id",
                (trip, track),
            ),
            ProposalPair::Waypoint { trip, waypoint } => (
                "trip_waypoint_link",
                "trip_id, waypoint_id",
                (trip, waypoint),
            ),
        };
        new += sqlx::query(&format!(
            "INSERT OR IGNORE INTO {table} \
             ({columns}, proposed, plan_covered, trip_on_plan, points_reached) \
             VALUES (?, ?, 1, ?, ?, ?)"
        ))
        .bind(ends.0)
        .bind(ends.1)
        .bind(proposal.plan_covered)
        .bind(proposal.trip_on_plan)
        .bind(proposal.points_reached)
        .execute(&mut *tx)
        .await?
        .rows_affected() as usize;
    }
    tx.commit().await?;
    Ok(new)
}

/// Forget what was proposed for these collection tracks and waypoints, their
/// links left as they are — for an item whose geometry a file replaced.
pub async fn forget_proposals_of(
    pool: &SqlitePool,
    track_ids: &[i64],
    waypoint_ids: &[i64],
) -> Result<(), sqlx::Error> {
    let mut tx = db::begin_write(pool).await?;
    for &id in track_ids {
        sqlx::query("DELETE FROM trip_track_link WHERE collection_track_id = ? AND proposed")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    for &id in waypoint_ids {
        sqlx::query("DELETE FROM trip_waypoint_link WHERE waypoint_id = ? AND proposed")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

/// How many proposals wait for the owner.
pub async fn count_proposals(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM trip_trip_link WHERE proposed) \
              + (SELECT COUNT(*) FROM trip_track_link WHERE proposed) \
              + (SELECT COUNT(*) FROM trip_waypoint_link WHERE proposed)",
    )
    .fetch_one(pool)
    .await
}

/// Confirm every pair, all or none: `false` if one is not a proposal, and
/// then nothing changed.
pub async fn confirm_proposals(pool: &SqlitePool, pairs: &[LinkPair]) -> Result<bool, sqlx::Error> {
    settle(pool, pairs, "UPDATE {table} SET proposed = 0").await
}

/// Dismiss every pair, all or none: `false` if one is not a proposal, and
/// then nothing changed. A dismissed proposal is not remembered.
pub async fn dismiss_proposals(pool: &SqlitePool, pairs: &[LinkPair]) -> Result<bool, sqlx::Error> {
    settle(pool, pairs, "DELETE FROM {table}").await
}

async fn settle(pool: &SqlitePool, pairs: &[LinkPair], action: &str) -> Result<bool, sqlx::Error> {
    let mut tx = db::begin_write(pool).await?;
    for pair in pairs {
        if !settle_one(&mut tx, pair, action).await? {
            return Ok(false);
        }
    }
    tx.commit().await?;
    Ok(true)
}

async fn settle_one(
    tx: &mut Transaction<'static, Sqlite>,
    pair: &LinkPair,
    action: &str,
) -> Result<bool, sqlx::Error> {
    let (table, condition) = match pair.other {
        LinkOther::Trip(_) => (
            "trip_trip_link",
            "((recorded_trip_id = ?1 AND planned_trip_id = ?2) \
              OR (recorded_trip_id = ?2 AND planned_trip_id = ?1))",
        ),
        LinkOther::CollectionTrack(_) => (
            "trip_track_link",
            "trip_id = ?1 AND collection_track_id = ?2",
        ),
        LinkOther::Waypoint(_) => ("trip_waypoint_link", "trip_id = ?1 AND waypoint_id = ?2"),
    };
    let other = match pair.other {
        LinkOther::Trip(id) | LinkOther::CollectionTrack(id) | LinkOther::Waypoint(id) => id,
    };
    let sql = format!(
        "{} WHERE {condition} AND proposed",
        action.replace("{table}", table)
    );
    let affected = sqlx::query(&sql)
        .bind(pair.trip_id)
        .bind(other)
        .execute(&mut **tx)
        .await?
        .rows_affected();
    Ok(affected > 0)
}

/// Every proposal, as the Proposed links page lists it: by their trips,
/// recorded trips newest first and then planned ones by name; a trip's
/// proposals planned trips first, then collection tracks, then waypoints,
/// each by name.
pub async fn list_proposals(pool: &SqlitePool) -> Result<Vec<ProposedLink>, sqlx::Error> {
    let mut listed: Vec<(Option<String>, u8, ProposedLink)> = Vec::new();
    let trip = |row: &SqliteRow| -> (LinkedTrip, TripKind, Option<String>) {
        let start_time: Option<String> = row.get("t_start");
        let tz_name: Option<String> = row.get("t_tz");
        (
            LinkedTrip {
                id: row.get("t_id"),
                name: row.get("t_name"),
                activity_type: row.get("t_activity"),
                start_date: local_start_date(start_time.as_deref(), tz_name.as_deref()),
                proposal: None,
            },
            row.get("t_kind"),
            start_time,
        )
    };
    const TRIP: &str = "t.id AS t_id, t.name AS t_name, t.activity_type AS t_activity, \
                        t.start_time AS t_start, t.tz_name AS t_tz, t.trip_kind AS t_kind, \
                        l.plan_covered, l.trip_on_plan, l.points_reached";

    let plans = sqlx::query(&format!(
        "SELECT {TRIP}, p.id, p.name, p.activity_type, p.start_time, p.tz_name \
         FROM trip_trip_link l JOIN trip t ON t.id = l.recorded_trip_id \
         JOIN trip p ON p.id = l.planned_trip_id WHERE l.proposed"
    ))
    .fetch_all(pool)
    .await?;
    for row in plans {
        let (trip, trip_kind, start) = trip(&row);
        let start_time: Option<String> = row.get("start_time");
        let tz_name: Option<String> = row.get("tz_name");
        let other = ProposedEnd::PlannedTrip(LinkedTrip {
            id: row.get("id"),
            name: row.get("name"),
            activity_type: row.get("activity_type"),
            start_date: local_start_date(start_time.as_deref(), tz_name.as_deref()),
            proposal: None,
        });
        listed.push((start, 0, entry(&row, trip, trip_kind, other)));
    }

    let tracks = sqlx::query(&format!(
        "SELECT {TRIP}, k.id, k.name, {TRACK_ACTIVITY} AS activity_type, \
                c.id AS collection_id, c.name AS collection_name \
         FROM trip_track_link l JOIN trip t ON t.id = l.trip_id \
         JOIN collection_track k ON k.id = l.collection_track_id \
         JOIN collection c ON c.id = k.collection_id WHERE l.proposed"
    ))
    .fetch_all(pool)
    .await?;
    for row in tracks {
        let (trip, trip_kind, start) = trip(&row);
        let other = ProposedEnd::CollectionTrack(LinkedTrack {
            id: row.get("id"),
            name: row.get("name"),
            activity_type: row.get("activity_type"),
            collection_id: row.get("collection_id"),
            collection_name: row.get("collection_name"),
            proposal: None,
        });
        listed.push((start, 1, entry(&row, trip, trip_kind, other)));
    }

    let waypoints = sqlx::query(&format!(
        "SELECT {TRIP}, w.id, w.name, w.symbol, \
                c.id AS collection_id, c.name AS collection_name \
         FROM trip_waypoint_link l JOIN trip t ON t.id = l.trip_id \
         JOIN waypoint w ON w.id = l.waypoint_id \
         JOIN collection c ON c.id = w.collection_id WHERE l.proposed"
    ))
    .fetch_all(pool)
    .await?;
    for row in waypoints {
        let (trip, trip_kind, start) = trip(&row);
        let other = ProposedEnd::Waypoint(LinkedWaypoint {
            id: row.get("id"),
            name: row.get("name"),
            symbol: row.get("symbol"),
            collection_id: row.get("collection_id"),
            collection_name: row.get("collection_name"),
            proposal: None,
        });
        listed.push((start, 2, entry(&row, trip, trip_kind, other)));
    }

    listed.sort_by(|(a_start, a_rank, a), (b_start, b_rank, b)| {
        let recorded = |link: &ProposedLink| link.trip_kind == TripKind::Recorded;
        recorded(b)
            .cmp(&recorded(a))
            .then_with(|| match recorded(a) {
                true => a_start
                    .is_none()
                    .cmp(&b_start.is_none())
                    .then_with(|| b_start.cmp(a_start)),
                false => by_name(&a.trip.name, &b.trip.name),
            })
            .then_with(|| b.trip.id.cmp(&a.trip.id))
            .then_with(|| a_rank.cmp(b_rank))
            .then_with(|| by_name(other_name(a), other_name(b)))
    });
    Ok(listed.into_iter().map(|(_, _, link)| link).collect())
}

fn entry(
    row: &SqliteRow,
    trip: LinkedTrip,
    trip_kind: TripKind,
    other: ProposedEnd,
) -> ProposedLink {
    ProposedLink {
        trip,
        trip_kind,
        other,
        proposal: Proposal {
            plan_covered: row.get("plan_covered"),
            trip_on_plan: row.get("trip_on_plan"),
            points_reached: row.get("points_reached"),
        },
    }
}

fn other_name(link: &ProposedLink) -> &str {
    match &link.other {
        ProposedEnd::PlannedTrip(trip) => &trip.name,
        ProposedEnd::CollectionTrack(track) => &track.name,
        ProposedEnd::Waypoint(waypoint) => waypoint.name.as_deref().unwrap_or(""),
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
