use super::*;
use crate::models::ItemDecision;
use crate::server::collection_gpx::{ParsedCollectionTrack, ParsedWaypoint};
use crate::server::gpx::TrackPoint;

/// About 11 m of latitude: within the tolerance.
const NUDGE: f64 = 0.0001;
/// About 110 m: beyond it.
const SHIFT: f64 = 0.001;

fn line(lat: f64) -> Vec<Position> {
    (0..20).map(|i| (lat, 9.2 + i as f64 * 0.001)).collect()
}

fn held_track(id: i64, name: &str, lat: f64) -> HeldTrack {
    HeldTrack {
        id,
        name: name.to_string(),
        line: line(lat),
    }
}

fn held_waypoint(id: i64, name: Option<&str>, lat: f64) -> HeldWaypoint {
    HeldWaypoint {
        id,
        name: name.map(str::to_string),
        description: Some("Hut".to_string()),
        lat,
        lon: 9.2,
        ele: Some(2000.0),
        symbol: Some("Lodge".to_string()),
    }
}

fn file_track(name: Option<&str>, lat: f64) -> ParsedCollectionTrack {
    ParsedCollectionTrack {
        position: 1,
        name: name.map(str::to_string),
        points: line(lat)
            .into_iter()
            .map(|(lat, lon)| TrackPoint {
                lat,
                lon,
                ele: None,
                time: None,
            })
            .collect(),
        gpx: Vec::new(),
    }
}

fn file_waypoint(name: Option<&str>, lat: f64) -> ParsedWaypoint {
    ParsedWaypoint {
        name: name.map(str::to_string),
        description: Some("Hut".to_string()),
        lat,
        lon: 9.2,
        ele: Some(2000.0),
        symbol: Some("Lodge".to_string()),
    }
}

fn file(tracks: Vec<ParsedCollectionTrack>, waypoints: Vec<ParsedWaypoint>) -> ParsedCollection {
    ParsedCollection {
        name: None,
        description: None,
        categories: Vec::new(),
        tracks,
        waypoints,
    }
}

fn actions(matched: &[Matched]) -> Vec<ImportAction> {
    matched.iter().map(|m| m.proposed).collect()
}

use ImportAction::{Add, Replace, Skip};

// ── Tracks ───────────────────────────────────────────────────────────────────

#[test]
fn us88_a_track_the_collection_holds_is_skipped_also_when_slightly_shifted() {
    let held = Held {
        tracks: vec![
            held_track(1, "Stage 1", 46.5),
            held_track(2, "Stage 2", 46.6),
        ],
        ..Held::default()
    };
    let files = [file(
        vec![
            file_track(Some("Stage 1"), 46.5),
            file_track(Some("Stage 2"), 46.6 + NUDGE),
        ],
        vec![],
    )];

    let matching = match_files(&held, &files, &Decisions::default());

    assert_eq!(actions(&matching.tracks), [Skip, Skip]);
    assert_eq!(matching.tracks[1].matched, Some(Origin::Held(2)));
    assert!(matching.unmatched_tracks.is_empty());
}

#[test]
fn us88_a_track_of_the_same_name_that_differs_is_replaced() {
    let held = Held {
        tracks: vec![held_track(1, "Stage 1", 46.5)],
        ..Held::default()
    };
    let files = [file(
        vec![file_track(Some("Stage 1"), 46.5 + SHIFT)],
        vec![],
    )];

    let matching = match_files(&held, &files, &Decisions::default());

    assert_eq!(actions(&matching.tracks), [Replace]);
    assert_eq!(
        matching.outcome.tracks,
        [FinalTrack {
            origin: Origin::Held(1),
            source: Some((0, 0)),
        }]
    );
}

#[test]
fn us88_a_track_nothing_matches_is_added_and_what_no_file_holds_is_kept() {
    let held = Held {
        tracks: vec![held_track(1, "Stage 1", 46.5)],
        ..Held::default()
    };
    // Same place, other name: a different track.
    let files = [file(vec![file_track(Some("Variante"), 46.5)], vec![])];

    let matching = match_files(&held, &files, &Decisions::default());

    assert_eq!(actions(&matching.tracks), [Add]);
    assert_eq!(matching.unmatched_tracks, [1]);
    assert_eq!(
        matching.outcome.tracks,
        [
            FinalTrack {
                origin: Origin::Held(1),
                source: None,
            },
            FinalTrack {
                origin: Origin::File { file: 0, index: 0 },
                source: Some((0, 0)),
            },
        ]
    );
    assert!(matching.outcome.remove_tracks.is_empty());
}

#[test]
fn us88_an_unnamed_track_is_matched_on_its_line_alone() {
    let held = Held {
        tracks: vec![held_track(1, "Lakes 2", 46.5)],
        ..Held::default()
    };
    let files = [file(
        vec![file_track(None, 46.5 + NUDGE), file_track(None, 47.0)],
        vec![],
    )];

    let matching = match_files(&held, &files, &Decisions::default());

    assert_eq!(actions(&matching.tracks), [Skip, Add]);
}

#[test]
fn us88_tracks_sharing_a_name_pair_with_the_closest() {
    let held = Held {
        tracks: vec![
            held_track(1, "Variante", 46.5),
            held_track(2, "Variante", 46.6),
        ],
        ..Held::default()
    };
    let files = [file(
        vec![
            file_track(Some("Variante"), 46.6),
            file_track(Some("Variante"), 46.5),
        ],
        vec![],
    )];

    let matching = match_files(&held, &files, &Decisions::default());

    assert_eq!(actions(&matching.tracks), [Skip, Skip]);
    assert_eq!(matching.tracks[0].matched, Some(Origin::Held(2)));
    assert_eq!(matching.tracks[1].matched, Some(Origin::Held(1)));
}

#[test]
fn us88_the_owner_decides_and_a_choice_that_is_not_offered_takes_the_proposal() {
    let held = Held {
        tracks: vec![held_track(1, "Stage 1", 46.5)],
        ..Held::default()
    };
    let files = [file(
        vec![
            file_track(Some("Stage 1"), 46.5),
            file_track(Some("New"), 47.0),
        ],
        vec![],
    )];
    let decisions = Decisions {
        tracks: HashMap::from([((0, 0), Add), ((0, 1), Replace)]),
        ..Decisions::default()
    };

    let matching = match_files(&held, &files, &decisions);

    assert_eq!(matching.tracks[0].action, Add);
    assert_eq!(matching.tracks[1].action, Add, "nothing to replace");
    assert_eq!(matching.outcome.tracks.len(), 3);
}

#[test]
fn us88_what_no_file_holds_is_removed_when_the_owner_says_so() {
    let held = Held {
        tracks: vec![held_track(1, "Stage 1", 46.5), held_track(2, "Old", 46.7)],
        waypoints: vec![held_waypoint(5, Some("Hut"), 46.5)],
    };
    let files = [file(vec![file_track(Some("Stage 1"), 46.5)], vec![])];
    let decisions = Decisions {
        // A held track the file matches stays whatever is asked.
        remove_tracks: HashSet::from([1, 2]),
        remove_waypoints: HashSet::from([5]),
        ..Decisions::default()
    };

    let matching = match_files(&held, &files, &decisions);

    assert_eq!(matching.outcome.remove_tracks, [2]);
    assert_eq!(matching.outcome.remove_waypoints, [5]);
    assert_eq!(
        matching.outcome.tracks,
        [FinalTrack {
            origin: Origin::Held(1),
            source: None,
        }]
    );
    assert!(matching.outcome.waypoints.is_empty());
}

#[test]
fn us88_replacing_the_whole_collection_brings_in_all_of_the_files() {
    let held = Held {
        tracks: vec![held_track(1, "Stage 1", 46.5)],
        waypoints: vec![held_waypoint(5, Some("Hut"), 46.5)],
    };
    let files = [file(
        vec![file_track(Some("Stage 1"), 46.5)],
        vec![file_waypoint(Some("Hut"), 46.5)],
    )];
    let decisions = Decisions {
        replace_all: true,
        ..Decisions::default()
    };

    let matching = match_files(&held, &files, &decisions);

    assert!(matching.outcome.replace_all);
    assert_eq!(actions(&matching.tracks), [Add]);
    assert_eq!(actions(&matching.waypoints), [Add]);
    assert!(matching.unmatched_tracks.is_empty());
    assert!(matching.unmatched_waypoints.is_empty());
}

// ── Waypoints ────────────────────────────────────────────────────────────────

#[test]
fn us88_a_waypoint_is_matched_by_name_and_position() {
    let held = Held {
        waypoints: vec![
            held_waypoint(1, Some("Hut"), 46.5),
            held_waypoint(2, Some("Pass"), 46.6),
            held_waypoint(3, None, 46.7),
        ],
        ..Held::default()
    };
    let files = [file(
        vec![],
        vec![
            file_waypoint(Some("Hut"), 46.5 + NUDGE),
            // Moved: the same name, somewhere else.
            file_waypoint(Some("Pass"), 46.6 + SHIFT),
            file_waypoint(None, 46.7),
            file_waypoint(Some("Lake"), 46.8),
        ],
    )];

    let matching = match_files(&held, &files, &Decisions::default());

    assert_eq!(actions(&matching.waypoints), [Skip, Replace, Skip, Add]);
}

#[test]
fn us88_a_waypoint_whose_details_differ_is_replaced() {
    let mut no_symbol = held_waypoint(1, Some("Hut"), 46.5);
    // Stored before US-95.
    no_symbol.symbol = None;
    let mut other_height = held_waypoint(2, Some("Pass"), 46.6);
    other_height.ele = Some(2010.0);
    let mut rounded = held_waypoint(3, Some("Lake"), 46.7);
    rounded.ele = Some(2000.4);
    let held = Held {
        waypoints: vec![no_symbol, other_height, rounded],
        ..Held::default()
    };
    let files = [file(
        vec![],
        vec![
            file_waypoint(Some("Hut"), 46.5),
            file_waypoint(Some("Pass"), 46.6),
            file_waypoint(Some("Lake"), 46.7),
        ],
    )];

    let matching = match_files(&held, &files, &Decisions::default());

    assert_eq!(actions(&matching.waypoints), [Replace, Replace, Skip]);
}

#[test]
fn us88_a_waypoint_takes_the_track_of_a_file_with_exactly_one() {
    let held = Held {
        tracks: vec![held_track(1, "Stage 1", 46.5)],
        waypoints: vec![held_waypoint(5, Some("Hut"), 46.5)],
    };
    let mut moved = file_waypoint(Some("Hut"), 46.5);
    moved.description = Some("Rebuilt".to_string());
    let files = [
        file(vec![file_track(Some("Stage 1"), 46.5)], vec![moved]),
        file(
            vec![file_track(Some("Stage 2"), 46.6)],
            vec![file_waypoint(Some("Lake"), 46.6)],
        ),
    ];

    let matching = match_files(&held, &files, &Decisions::default());

    assert_eq!(
        matching.outcome.waypoints,
        [
            FinalWaypoint {
                origin: Origin::Held(5),
                source: Some((0, 0)),
                track: Membership::Track(Some(Origin::Held(1))),
            },
            FinalWaypoint {
                origin: Origin::File { file: 1, index: 0 },
                source: Some((1, 0)),
                track: Membership::Track(Some(Origin::File { file: 1, index: 0 })),
            },
        ]
    );
}

#[test]
fn us88_a_waypoint_replaced_from_a_file_of_many_tracks_keeps_its_track() {
    let held = Held {
        tracks: vec![held_track(1, "Stage 1", 46.5)],
        waypoints: vec![held_waypoint(5, Some("Hut"), 46.5)],
    };
    let mut moved = file_waypoint(Some("Hut"), 46.5);
    moved.description = Some("Rebuilt".to_string());
    let files = [file(
        vec![
            file_track(Some("Stage 1"), 46.5),
            file_track(Some("Stage 2"), 46.6),
        ],
        vec![moved],
    )];

    let matching = match_files(&held, &files, &Decisions::default());

    assert_eq!(matching.outcome.waypoints[0].track, Membership::Unchanged);
}

// ── Several files ────────────────────────────────────────────────────────────

#[test]
fn us88_an_item_in_two_files_is_added_once() {
    let files = [
        file(
            vec![file_track(Some("Stage 1"), 46.5)],
            vec![file_waypoint(Some("Hut"), 46.5)],
        ),
        file(
            vec![
                file_track(Some("Stage 1"), 46.5 + NUDGE),
                file_track(Some("Stage 2"), 46.6),
            ],
            vec![file_waypoint(Some("Hut"), 46.5 + SHIFT)],
        ),
    ];

    let matching = match_files(&Held::default(), &files, &Decisions::default());

    assert_eq!(actions(&matching.tracks), [Add, Skip, Add]);
    assert_eq!(
        matching.tracks[1].matched,
        Some(Origin::File { file: 0, index: 0 })
    );
    assert_eq!(actions(&matching.waypoints), [Add, Replace]);
    // The later file's waypoint replaces the earlier one's: one waypoint.
    assert_eq!(
        matching.outcome.waypoints,
        [FinalWaypoint {
            origin: Origin::File { file: 0, index: 0 },
            source: Some((1, 0)),
            track: Membership::Track(Some(Origin::File { file: 0, index: 0 })),
        }]
    );
}

#[test]
fn us88_an_earlier_files_decision_changes_what_a_later_one_is_proposed() {
    let files = [
        file(vec![file_track(Some("Stage 1"), 46.5)], vec![]),
        file(vec![file_track(Some("Stage 1"), 46.5)], vec![]),
    ];
    let decisions = Decisions {
        tracks: HashMap::from([((0, 0), Skip)]),
        ..Decisions::default()
    };

    let matching = match_files(&Held::default(), &files, &decisions);

    assert_eq!(actions(&matching.tracks), [Add, Add]);
    assert_eq!(matching.tracks[0].action, Skip);
    assert_eq!(
        matching.outcome.tracks,
        [FinalTrack {
            origin: Origin::File { file: 1, index: 0 },
            source: Some((1, 0)),
        }]
    );
}

#[test]
fn us88_only_what_was_held_before_the_upload_is_offered_for_removal() {
    let held = Held {
        tracks: vec![held_track(1, "Stage 1", 46.5), held_track(2, "Old", 46.9)],
        ..Held::default()
    };
    let files = [
        file(vec![file_track(Some("New"), 47.0)], vec![]),
        file(vec![file_track(Some("Stage 1"), 46.5)], vec![]),
    ];

    let matching = match_files(&held, &files, &Decisions::default());

    // The first file's track is not in the second, and still not offered.
    assert_eq!(matching.unmatched_tracks, [2]);
}

// ── The digest ───────────────────────────────────────────────────────────────

#[test]
fn us88_the_digest_changes_with_what_the_collection_holds() {
    let held = || Held {
        tracks: vec![held_track(1, "Stage 1", 46.5)],
        waypoints: vec![held_waypoint(5, Some("Hut"), 46.5)],
    };
    let mut renamed = held();
    renamed.tracks[0].name = "Stage 1a".to_string();
    let mut resymbolled = held();
    resymbolled.waypoints[0].symbol = None;

    assert_eq!(held().digest(), held().digest());
    assert_ne!(held().digest(), renamed.digest());
    assert_ne!(held().digest(), resymbolled.digest());
}

#[test]
fn us88_decisions_from_the_wire_are_keyed_by_file_and_item() {
    let wire = ImportDecisions {
        replace_all: false,
        tracks: vec![ItemDecision {
            file: 1,
            index: 2,
            action: Skip,
        }],
        waypoints: vec![],
        remove_tracks: vec![7],
        remove_waypoints: vec![],
    };

    let decisions = Decisions::from(&wire);

    assert_eq!(decisions.tracks.get(&(1, 2)), Some(&Skip));
    assert!(decisions.remove_tracks.contains(&7));
}
