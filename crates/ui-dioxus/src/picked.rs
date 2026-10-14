//! What is selected on the trip list (US-90): trips, collections and
//! collection tracks, together, across pages and kinds — and which of the
//! actions on a selection (US-34/53/63/94/99/100) can act on it.
//!
//! Pure and Dioxus-free, so the rules are unit-tested on the host.

use std::collections::BTreeSet;

use crate::kinds::{Kind, Kinds};
use crate::share::Selected;

/// One selected row. A trip keeps its kind, so turning that kind off drops
/// it from the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Picked {
    Trip(Kind, i64),
    Collection(i64),
    Track(i64),
}

impl Picked {
    /// The toggle that lists it.
    fn kind(self) -> Kind {
        match self {
            Picked::Trip(kind, _) => kind,
            Picked::Collection(_) | Picked::Track(_) => Kind::Collections,
        }
    }

    /// What it is as the actions see it: recorded and planned trips alike.
    fn of(self) -> Selected {
        match self {
            Picked::Trip(..) => Selected::Trips,
            Picked::Collection(_) => Selected::Collections,
            Picked::Track(_) => Selected::CollectionTracks,
        }
    }

    fn id(self) -> i64 {
        match self {
            Picked::Trip(_, id) | Picked::Collection(id) | Picked::Track(id) => id,
        }
    }
}

/// The selection, for an action to act on: everything selected is one of
/// trips, collections or collection tracks.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub of: Selected,
    pub ids: Vec<i64>,
}

/// The selection as one [`Target`] — `None` for nothing selected, or for a
/// mix that no action takes.
pub fn target(picked: &BTreeSet<Picked>) -> Option<Target> {
    let of = picked.first()?.of();
    picked.iter().all(|p| p.of() == of).then(|| Target {
        of,
        ids: picked.iter().map(|p| p.id()).collect(),
    })
}

/// What can be done with a selection, in the order the bar offers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Tag,
    Share,
    SetActivity,
    Link,
}

impl Action {
    pub const ALL: [Action; 4] = [
        Action::Tag,
        Action::Share,
        Action::SetActivity,
        Action::Link,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::Tag => "Tag…",
            Action::Share => "Share…",
            Action::SetActivity => "Set activity…",
            Action::Link => "Link…",
        }
    }

    /// What to select for it to be taken, said while it cannot be.
    pub fn needs(self) -> &'static str {
        match self {
            Action::Tag => "Select only trips, or only collections",
            Action::Share => "Select only trips, only collections, or only collection tracks",
            Action::SetActivity => "Select only trips",
            Action::Link => {
                "Select recorded trips with planned trips or collection tracks, \
                 or planned trips with collection tracks — and no collections"
            }
        }
    }

    /// Whether it acts on what `target` holds, as it does today: tags go on
    /// trips and collections (US-34/100), shares take any of the three
    /// (US-53/94), and only trips have their activity set in bulk (US-63).
    /// Linking acts on no single sort but on pairs of them.
    fn acts_on(self, of: Selected) -> bool {
        match self {
            Action::Tag => matches!(of, Selected::Trips | Selected::Collections),
            Action::Share => true,
            Action::SetActivity => of == Selected::Trips,
            Action::Link => false,
        }
    }

    /// Whether it can be taken on `picked`: something is selected, all of
    /// one sort it acts on — or, for linking, a pair that can be linked and
    /// no collection, which nothing is linked to (US-99).
    pub fn active(self, picked: &BTreeSet<Picked>) -> bool {
        match self {
            Action::Link => {
                link_pairs(picked) > 0 && !picked.iter().any(|p| matches!(p, Picked::Collection(_)))
            }
            _ => target(picked).is_some_and(|target| self.acts_on(target.of)),
        }
    }
}

/// The pairs the selection holds that can be linked (US-99): every recorded
/// trip with every planned trip and collection track, every planned trip
/// with every collection track.
pub fn link_pairs(picked: &BTreeSet<Picked>) -> usize {
    let count = |wanted: fn(&Picked) -> bool| picked.iter().filter(|p| wanted(p)).count();
    let recorded = count(|p| matches!(p, Picked::Trip(Kind::Recorded, _)));
    let planned = count(|p| matches!(p, Picked::Trip(Kind::Planned, _)));
    let tracks = count(|p| matches!(p, Picked::Track(_)));
    recorded * planned + (recorded + planned) * tracks
}

/// The selected trips and collection tracks, for linking them.
pub fn link_ends(picked: &BTreeSet<Picked>) -> (Vec<i64>, Vec<i64>) {
    let trips = picked
        .iter()
        .filter_map(|p| match p {
            Picked::Trip(_, id) => Some(*id),
            _ => None,
        })
        .collect();
    let tracks = picked
        .iter()
        .filter_map(|p| match p {
            Picked::Track(id) => Some(*id),
            _ => None,
        })
        .collect();
    (trips, tracks)
}

/// The bar's summary: how many are selected, and of what.
pub fn summary(picked: &BTreeSet<Picked>) -> String {
    let count = |of: Selected| picked.iter().filter(|p| p.of() == of).count();
    let parts: Vec<String> = [
        (Selected::Trips, "trip", "trips"),
        (Selected::Collections, "collection", "collections"),
        (
            Selected::CollectionTracks,
            "collection track",
            "collection tracks",
        ),
    ]
    .into_iter()
    .map(|(of, one, many)| (count(of), one, many))
    .filter(|(n, _, _)| *n > 0)
    .map(|(n, one, many)| format!("{n} {}", if n == 1 { one } else { many }))
    .collect();
    format!("{} selected: {}", picked.len(), parts.join(" · "))
}

/// Drops what the kinds now off list: an action never reaches a row that
/// is not shown.
pub fn retain_kinds(picked: &mut BTreeSet<Picked>, kinds: Kinds) {
    picked.retain(|p| kinds.contains(p.kind()));
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn set(picked: &[Picked]) -> BTreeSet<Picked> {
        picked.iter().copied().collect()
    }

    #[test]
    fn us90_recorded_and_planned_trips_are_one_target() {
        let picked = set(&[
            Picked::Trip(Kind::Recorded, 1),
            Picked::Trip(Kind::Planned, 2),
        ]);

        assert_eq!(
            target(&picked),
            Some(Target {
                of: Selected::Trips,
                ids: vec![1, 2],
            })
        );
    }

    #[test]
    fn us90_a_mixed_selection_or_none_is_no_target() {
        assert_eq!(target(&BTreeSet::new()), None);
        assert_eq!(
            target(&set(&[Picked::Trip(Kind::Recorded, 1), Picked::Track(1)])),
            None
        );
        assert_eq!(
            target(&set(&[Picked::Collection(1), Picked::Track(2)])),
            None
        );
    }

    #[test]
    fn us90_each_action_is_active_only_on_what_it_acts_on_today() {
        let trips = set(&[Picked::Trip(Kind::Planned, 1)]);
        let collections = set(&[Picked::Collection(1)]);
        let tracks = set(&[Picked::Track(1)]);
        let mixed = set(&[Picked::Trip(Kind::Recorded, 1), Picked::Collection(1)]);

        let active = |picked: &BTreeSet<Picked>| -> Vec<Action> {
            Action::ALL
                .into_iter()
                .filter(|action| action.active(picked))
                .collect()
        };
        assert_eq!(
            active(&trips),
            [Action::Tag, Action::Share, Action::SetActivity]
        );
        assert_eq!(
            active(&set(&[Picked::Trip(Kind::Planned, 1), Picked::Track(2)])),
            [Action::Link]
        );
        assert_eq!(active(&collections), [Action::Tag, Action::Share]);
        assert_eq!(active(&tracks), [Action::Share]);
        assert_eq!(active(&mixed), []);
        assert_eq!(active(&BTreeSet::new()), []);
    }

    #[test]
    fn us99_every_pair_that_can_be_linked_is_counted() {
        let mut picked = set(&[Picked::Trip(Kind::Planned, 1)]);
        for day in 2..=7 {
            picked.insert(Picked::Trip(Kind::Recorded, day));
        }
        // Six days and one plan: six links.
        assert_eq!(link_pairs(&picked), 6);
        picked.insert(Picked::Track(1));
        picked.insert(Picked::Track(2));
        // And each of the seven trips with each of two tracks.
        assert_eq!(link_pairs(&picked), 6 + 7 * 2);
        assert_eq!(link_ends(&picked), (vec![2, 3, 4, 5, 6, 7, 1], vec![1, 2]));
    }

    #[test]
    fn us99_linking_needs_a_pair_and_no_collection() {
        let linkable = set(&[
            Picked::Trip(Kind::Recorded, 1),
            Picked::Trip(Kind::Planned, 2),
        ]);
        let mut with_a_collection = linkable.clone();
        with_a_collection.insert(Picked::Collection(3));

        assert!(Action::Link.active(&linkable));
        assert!(!Action::Link.active(&with_a_collection));
        for unlinkable in [
            set(&[
                Picked::Trip(Kind::Recorded, 1),
                Picked::Trip(Kind::Recorded, 2),
            ]),
            set(&[
                Picked::Trip(Kind::Planned, 1),
                Picked::Trip(Kind::Planned, 2),
            ]),
            set(&[Picked::Track(1), Picked::Track(2)]),
            set(&[Picked::Trip(Kind::Recorded, 1)]),
        ] {
            assert_eq!(link_pairs(&unlinkable), 0);
            assert!(!Action::Link.active(&unlinkable));
        }
    }

    #[test]
    fn us90_the_summary_counts_trips_collections_and_tracks() {
        let picked = set(&[
            Picked::Trip(Kind::Recorded, 1),
            Picked::Trip(Kind::Planned, 2),
            Picked::Track(3),
        ]);

        assert_eq!(summary(&picked), "3 selected: 2 trips · 1 collection track");
        assert_eq!(
            summary(&set(&[Picked::Collection(1)])),
            "1 selected: 1 collection"
        );
    }

    #[test]
    fn us90_switching_a_kind_off_drops_what_it_listed() {
        let mut picked = set(&[
            Picked::Trip(Kind::Recorded, 1),
            Picked::Trip(Kind::Planned, 2),
            Picked::Collection(3),
            Picked::Track(4),
        ]);

        retain_kinds(&mut picked, Kinds::only(Kind::Planned));

        assert_eq!(picked, set(&[Picked::Trip(Kind::Planned, 2)]));
    }

    #[test]
    fn a_trip_and_a_collection_track_with_one_id_are_told_apart() {
        let picked = set(&[Picked::Trip(Kind::Recorded, 7), Picked::Track(7)]);

        assert_eq!(picked.len(), 2);
    }
}
