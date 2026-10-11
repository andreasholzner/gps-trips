//! Matching uploaded files against what a collection holds (US-88,
//! ADR-0029): which of their tracks and waypoints the collection already
//! has, which differ, and which are new — proposed, never decided here.
//!
//! The files are taken one after another, each against the collection as
//! the files before it and the owner's decisions on them leave it, so
//! uploading several at once ends where adding them one by one would.
//! Pure: what the collection holds comes in, what it would hold goes out,
//! and storing that is the caller's (`repo::apply_collection_import`).

use std::collections::{HashMap, HashSet};

use crate::config::collection_match::{
    ELEVATION_TOLERANCE_M, TRACK_TOLERANCE_M, WAYPOINT_TOLERANCE_M,
};
use crate::models::{ImportAction, ImportDecisions};
use crate::server::collection_gpx::ParsedCollection;

mod geometry;

use geometry::{centre, distance_m, lines_within, Position};

/// A track the collection holds.
pub struct HeldTrack {
    pub id: i64,
    pub name: String,
    pub line: Vec<Position>,
}

/// A waypoint the collection holds.
pub struct HeldWaypoint {
    pub id: i64,
    pub name: Option<String>,
    pub description: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    pub symbol: Option<String>,
}

/// Everything a collection holds, in its order.
#[derive(Default)]
pub struct Held {
    pub tracks: Vec<HeldTrack>,
    pub waypoints: Vec<HeldWaypoint>,
}

impl Held {
    /// A fingerprint of everything matching reads, so a confirmation can
    /// tell that the collection changed since the owner's review.
    pub fn digest(&self) -> String {
        let mut hash = Fnv::default();
        for track in &self.tracks {
            hash.write_i64(track.id);
            hash.write_str(Some(&track.name));
            for &(lat, lon) in &track.line {
                hash.write_f64(Some(lat));
                hash.write_f64(Some(lon));
            }
        }
        hash.write_str(Some("waypoints"));
        for wp in &self.waypoints {
            hash.write_i64(wp.id);
            hash.write_str(wp.name.as_deref());
            hash.write_str(wp.description.as_deref());
            hash.write_f64(Some(wp.lat));
            hash.write_f64(Some(wp.lon));
            hash.write_f64(wp.ele);
            hash.write_str(wp.symbol.as_deref());
        }
        format!("{:016x}", hash.0)
    }
}

/// Where a track or waypoint of the outcome comes from: the collection, or
/// the first file item that added it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    Held(i64),
    File { file: usize, index: usize },
}

/// A file's track or waypoint: the file's index and its own.
pub type ItemKey = (usize, usize);

/// The owner's decisions, keyed for looking up.
#[derive(Default)]
pub struct Decisions {
    pub replace_all: bool,
    pub tracks: HashMap<ItemKey, ImportAction>,
    pub waypoints: HashMap<ItemKey, ImportAction>,
    pub remove_tracks: HashSet<i64>,
    pub remove_waypoints: HashSet<i64>,
}

impl From<&ImportDecisions> for Decisions {
    fn from(decisions: &ImportDecisions) -> Self {
        let keyed = |items: &[crate::models::ItemDecision]| {
            items
                .iter()
                .map(|item| ((item.file, item.index), item.action))
                .collect()
        };
        Self {
            replace_all: decisions.replace_all,
            tracks: keyed(&decisions.tracks),
            waypoints: keyed(&decisions.waypoints),
            remove_tracks: decisions.remove_tracks.iter().copied().collect(),
            remove_waypoints: decisions.remove_waypoints.iter().copied().collect(),
        }
    }
}

/// One file item, with what it matched and what becomes of it.
#[derive(Debug, PartialEq)]
pub struct Matched {
    pub file: usize,
    pub index: usize,
    pub matched: Option<Origin>,
    pub proposed: ImportAction,
    pub action: ImportAction,
}

impl Matched {
    /// What the owner may choose: replacing needs something to replace.
    pub fn choices(&self) -> &'static [ImportAction] {
        if self.matched.is_some() {
            &[ImportAction::Skip, ImportAction::Replace, ImportAction::Add]
        } else {
            &[ImportAction::Add, ImportAction::Skip]
        }
    }
}

/// Which track a waypoint of the outcome belongs to as well.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Membership {
    /// As the collection has it.
    Unchanged,
    Track(Option<Origin>),
}

/// A track of the outcome; `source` is the file item it now takes its
/// contents from, `None` for one left as it is.
#[derive(Debug, PartialEq)]
pub struct FinalTrack {
    pub origin: Origin,
    pub source: Option<ItemKey>,
}

#[derive(Debug, PartialEq)]
pub struct FinalWaypoint {
    pub origin: Origin,
    pub source: Option<ItemKey>,
    pub track: Membership,
}

/// What the collection would hold: the held tracks and waypoints first, in
/// their order, then the added ones in the files'.
#[derive(Debug, PartialEq)]
pub struct Outcome {
    /// Everything held goes first; `tracks` and `waypoints` are then all new.
    pub replace_all: bool,
    pub tracks: Vec<FinalTrack>,
    pub waypoints: Vec<FinalWaypoint>,
    pub remove_tracks: Vec<i64>,
    pub remove_waypoints: Vec<i64>,
}

/// The review: every file item in file order, what the collection holds and
/// none of the files does, and where it all ends.
pub struct Matching {
    pub tracks: Vec<Matched>,
    pub waypoints: Vec<Matched>,
    pub unmatched_tracks: Vec<i64>,
    pub unmatched_waypoints: Vec<i64>,
    pub outcome: Outcome,
}

struct WorkTrack<'a> {
    origin: Origin,
    name: Option<&'a str>,
    line: &'a [Position],
    source: Option<ItemKey>,
}

struct WorkWaypoint<'a> {
    origin: Origin,
    name: Option<&'a str>,
    description: Option<&'a str>,
    position: Position,
    ele: Option<f64>,
    symbol: Option<&'a str>,
    source: Option<ItemKey>,
    track: Membership,
}

/// Match `files`, in order, against `held` and apply `decisions` as they go.
/// A decision that is not among an item's choices takes the proposal.
pub fn match_files(held: &Held, files: &[ParsedCollection], decisions: &Decisions) -> Matching {
    let lines: Vec<Vec<Vec<Position>>> = files
        .iter()
        .map(|file| {
            file.tracks
                .iter()
                .map(|track| track.points.iter().map(|p| (p.lat, p.lon)).collect())
                .collect()
        })
        .collect();

    let (mut tracks, mut waypoints) = if decisions.replace_all {
        (Vec::new(), Vec::new())
    } else {
        (held_tracks(held), held_waypoints(held))
    };
    let mut matched_held = HashSet::new();
    let mut review_tracks = Vec::new();
    let mut review_waypoints = Vec::new();

    for (f, file) in files.iter().enumerate() {
        let candidates: Vec<WorkTrack> = file
            .tracks
            .iter()
            .enumerate()
            .map(|(i, track)| WorkTrack {
                origin: Origin::File { file: f, index: i },
                name: track.name.as_deref(),
                line: &lines[f][i],
                source: Some((f, i)),
            })
            .collect();
        let pairs = pair(candidates.len(), tracks.len(), |i, j| {
            track_relation(&candidates[i], &tracks[j])
        });
        let mut results = Vec::with_capacity(candidates.len());
        for (i, (candidate, relation)) in candidates.into_iter().zip(pairs).enumerate() {
            let matched = relation.map(|(j, _)| (j, tracks[j].origin));
            let proposed = match relation {
                None => ImportAction::Add,
                Some((_, Rank::Same)) => ImportAction::Skip,
                Some(_) => ImportAction::Replace,
            };
            let item = decided((f, i), matched.map(|(_, o)| o), proposed, &decisions.tracks);
            if let Some((_, Origin::Held(id))) = matched {
                matched_held.insert(id);
            }
            let result = match (item.action, matched) {
                (ImportAction::Add, _) => {
                    let origin = candidate.origin;
                    tracks.push(candidate);
                    Some(origin)
                }
                (ImportAction::Replace, Some((j, origin))) => {
                    tracks[j].name = candidate.name.or(tracks[j].name);
                    tracks[j].line = candidate.line;
                    tracks[j].source = candidate.source;
                    Some(origin)
                }
                (_, matched) => matched.map(|(_, origin)| origin),
            };
            results.push(result);
            review_tracks.push(item);
        }
        // US-86: a file with exactly one track gives it its waypoints.
        let file_track = (file.tracks.len() == 1).then(|| results[0]);

        let candidates: Vec<WorkWaypoint> = file
            .waypoints
            .iter()
            .enumerate()
            .map(|(i, wp)| WorkWaypoint {
                origin: Origin::File { file: f, index: i },
                name: wp.name.as_deref(),
                description: wp.description.as_deref(),
                position: (wp.lat, wp.lon),
                ele: wp.ele,
                symbol: wp.symbol.as_deref(),
                source: Some((f, i)),
                track: Membership::Track(file_track.flatten()),
            })
            .collect();
        let pairs = pair(candidates.len(), waypoints.len(), |i, j| {
            waypoint_relation(&candidates[i], &waypoints[j])
        });
        for (i, (candidate, relation)) in candidates.into_iter().zip(pairs).enumerate() {
            let matched = relation.map(|(j, rank)| (j, waypoints[j].origin, rank));
            let proposed = match matched {
                None => ImportAction::Add,
                Some((j, _, Rank::Same)) if same_details(&candidate, &waypoints[j]) => {
                    ImportAction::Skip
                }
                Some(_) => ImportAction::Replace,
            };
            let item = decided(
                (f, i),
                matched.map(|(_, o, _)| o),
                proposed,
                &decisions.waypoints,
            );
            if let Some((_, Origin::Held(id), _)) = matched {
                matched_held.insert(id);
            }
            match (item.action, matched) {
                (ImportAction::Add, _) => waypoints.push(candidate),
                (ImportAction::Replace, Some((j, _, _))) => {
                    let target = &mut waypoints[j];
                    target.name = candidate.name.or(target.name);
                    target.description = candidate.description;
                    target.position = candidate.position;
                    target.ele = candidate.ele;
                    target.symbol = candidate.symbol;
                    target.source = candidate.source;
                    if file_track.is_some() {
                        target.track = candidate.track;
                    }
                }
                _ => {}
            }
            review_waypoints.push(item);
        }
    }

    let unmatched = |ids: Vec<i64>| -> Vec<i64> {
        ids.into_iter()
            .filter(|id| !matched_held.contains(id))
            .collect()
    };
    let (unmatched_tracks, unmatched_waypoints) = if decisions.replace_all {
        (Vec::new(), Vec::new())
    } else {
        (
            unmatched(held.tracks.iter().map(|t| t.id).collect()),
            unmatched(held.waypoints.iter().map(|w| w.id).collect()),
        )
    };
    let removed = |ids: &[i64], remove: &HashSet<i64>| -> Vec<i64> {
        ids.iter()
            .copied()
            .filter(|id| remove.contains(id))
            .collect()
    };
    let remove_tracks = removed(&unmatched_tracks, &decisions.remove_tracks);
    let remove_waypoints = removed(&unmatched_waypoints, &decisions.remove_waypoints);
    let outcome = Outcome {
        replace_all: decisions.replace_all,
        tracks: tracks
            .into_iter()
            .filter(|t| !matches!(t.origin, Origin::Held(id) if remove_tracks.contains(&id)))
            .map(|t| FinalTrack {
                origin: t.origin,
                source: t.source,
            })
            .collect(),
        waypoints: waypoints
            .into_iter()
            .filter(|w| !matches!(w.origin, Origin::Held(id) if remove_waypoints.contains(&id)))
            .map(|w| FinalWaypoint {
                origin: w.origin,
                source: w.source,
                track: w.track,
            })
            .collect(),
        remove_tracks,
        remove_waypoints,
    };

    Matching {
        tracks: review_tracks,
        waypoints: review_waypoints,
        unmatched_tracks,
        unmatched_waypoints,
        outcome,
    }
}

fn held_tracks(held: &Held) -> Vec<WorkTrack<'_>> {
    held.tracks
        .iter()
        .map(|track| WorkTrack {
            origin: Origin::Held(track.id),
            name: Some(&track.name),
            line: &track.line,
            source: None,
        })
        .collect()
}

fn held_waypoints(held: &Held) -> Vec<WorkWaypoint<'_>> {
    held.waypoints
        .iter()
        .map(|wp| WorkWaypoint {
            origin: Origin::Held(wp.id),
            name: wp.name.as_deref(),
            description: wp.description.as_deref(),
            position: (wp.lat, wp.lon),
            ele: wp.ele,
            symbol: wp.symbol.as_deref(),
            source: None,
            track: Membership::Unchanged,
        })
        .collect()
}

/// The owner's decision where it is one of the item's choices, else the
/// proposal.
fn decided(
    (file, index): ItemKey,
    matched: Option<Origin>,
    proposed: ImportAction,
    decisions: &HashMap<ItemKey, ImportAction>,
) -> Matched {
    let mut item = Matched {
        file,
        index,
        matched,
        proposed,
        action: proposed,
    };
    if let Some(&action) = decisions.get(&(file, index)) {
        if item.choices().contains(&action) {
            item.action = action;
        }
    }
    item
}

/// How a file item relates to one the collection would hold: the same,
/// within the tolerance, or one of the same name beyond it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Same,
    Changed,
}

/// Same name — or none in the file, and then within the tolerance only.
fn track_relation(file: &WorkTrack, held: &WorkTrack) -> Option<(Rank, f64)> {
    let same_name = file.name.is_some() && file.name == held.name;
    if !same_name && file.name.is_some() {
        return None;
    }
    let distance = match (centre(file.line), centre(held.line)) {
        (Some(a), Some(b)) => distance_m(a, b),
        _ => f64::INFINITY,
    };
    if lines_within(file.line, held.line, TRACK_TOLERANCE_M) {
        Some((Rank::Same, distance))
    } else {
        same_name.then_some((Rank::Changed, distance))
    }
}

fn waypoint_relation(file: &WorkWaypoint, held: &WorkWaypoint) -> Option<(Rank, f64)> {
    let same_name = file.name.is_some() && file.name == held.name;
    if !same_name && file.name.is_some() {
        return None;
    }
    let distance = distance_m(file.position, held.position);
    if distance <= WAYPOINT_TOLERANCE_M {
        Some((Rank::Same, distance))
    } else {
        same_name.then_some((Rank::Changed, distance))
    }
}

/// Everything but the name and the place (ADR-0029). A symbol stored before
/// US-95 is none, and so differs from the file's.
fn same_details(file: &WorkWaypoint, held: &WorkWaypoint) -> bool {
    let ele = match (file.ele, held.ele) {
        (Some(a), Some(b)) => (a - b).abs() <= ELEVATION_TOLERANCE_M,
        (a, b) => a.is_none() && b.is_none(),
    };
    ele && file.description == held.description && file.symbol == held.symbol
}

/// Pair each of `items` items with at most one of `entries`, the closest
/// relations first; each entry is paired at most once.
fn pair(
    items: usize,
    entries: usize,
    relation: impl Fn(usize, usize) -> Option<(Rank, f64)>,
) -> Vec<Option<(usize, Rank)>> {
    let mut candidates: Vec<(Rank, f64, usize, usize)> = (0..items)
        .flat_map(|i| (0..entries).map(move |j| (i, j)))
        .filter_map(|(i, j)| relation(i, j).map(|(rank, distance)| (rank, distance, i, j)))
        .collect();
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut paired = vec![None; items];
    let mut taken = vec![false; entries];
    for (rank, _, i, j) in candidates {
        if paired[i].is_none() && !taken[j] {
            paired[i] = Some((j, rank));
            taken[j] = true;
        }
    }
    paired
}

/// FNV-1a: stable across builds, unlike the standard hasher.
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn write_i64(&mut self, value: i64) {
        self.write(&value.to_le_bytes());
    }

    fn write_f64(&mut self, value: Option<f64>) {
        self.write(&value.map_or(u64::MAX, f64::to_bits).to_le_bytes());
    }

    /// Length first, so `("ab", "c")` and `("a", "bc")` differ; none apart
    /// from empty.
    fn write_str(&mut self, value: Option<&str>) {
        match value {
            Some(text) => {
                self.write_i64(text.len() as i64);
                self.write(text.as_bytes());
            }
            None => self.write_i64(-1),
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests;
