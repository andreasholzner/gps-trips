//! What the trip list lists and maps (US-90): any of recorded trips, planned
//! trips (US-32) and collections (US-87), chosen by toggles that replaced
//! the tabs. Never none: the last one on stays on.
//!
//! Pure and Dioxus-free, so the rules are unit-tested on the host.

use trip_archive_types::TripKind;

/// One kind of thing the list holds. Not a [`TripKind`]: collections are not
/// trips (ADR-0028), and the server's trip kinds must never hear of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    #[default]
    Recorded,
    Planned,
    Collections,
}

impl Kind {
    /// In the order the toggles are shown.
    pub const ALL: [Kind; 3] = [Self::Recorded, Self::Planned, Self::Collections];

    /// The kind's value in the SPA's `kind` parameter (US-52).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::Planned => "planned",
            Self::Collections => "collections",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Recorded => "Recorded",
            Self::Planned => "Planned",
            Self::Collections => "Collections",
        }
    }

    /// The kind of trip it is; `None` for collections.
    pub fn trip_kind(self) -> Option<TripKind> {
        match self {
            Self::Recorded => Some(TripKind::Recorded),
            Self::Planned => Some(TripKind::Planned),
            Self::Collections => None,
        }
    }

    /// The list's kind of a trip of `kind`.
    pub fn of_trip(kind: TripKind) -> Self {
        match kind {
            TripKind::Recorded => Self::Recorded,
            TripKind::Planned => Self::Planned,
        }
    }

    /// How its lines are drawn (US-90), as an SVG and Leaflet `dashArray`:
    /// recorded solid, planned dashed, collection tracks dotted — the dots
    /// are dashes shorter than the line is wide, rounded by its caps.
    pub fn dash(self) -> Option<&'static str> {
        match self {
            Self::Recorded => None,
            Self::Planned => Some("8 6"),
            Self::Collections => Some("1 7"),
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The kinds that are on. At least one always is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kinds {
    on: [bool; 3],
}

impl Default for Kinds {
    /// Recorded trips alone, as "All trips" opens the list (US-60).
    fn default() -> Self {
        Self::only(Kind::Recorded)
    }
}

impl Kinds {
    pub fn only(kind: Kind) -> Self {
        let mut on = [false; 3];
        on[kind.index()] = true;
        Self { on }
    }

    pub fn contains(self, kind: Kind) -> bool {
        self.on[kind.index()]
    }

    /// The kinds that are on, in toggle order.
    pub fn iter(self) -> impl Iterator<Item = Kind> {
        Kind::ALL
            .into_iter()
            .filter(move |kind| self.contains(*kind))
    }

    /// Whether `kind` is the only one on, so switching it off is refused.
    pub fn is_last(self, kind: Kind) -> bool {
        self.contains(kind) && self.iter().count() == 1
    }

    /// `kind` switched on or off — unless it is the last one on.
    pub fn toggled(self, kind: Kind) -> Self {
        if self.is_last(kind) {
            return self;
        }
        let mut on = self.on;
        on[kind.index()] = !on[kind.index()];
        Self { on }
    }

    /// Whether any kind of trip is on.
    pub fn has_trips(self) -> bool {
        self.contains(Kind::Recorded) || self.contains(Kind::Planned)
    }

    /// The kinds of trip on, collections left out — what the trips are read
    /// for, so toggling collections does not read them again — or `None`
    /// with no kind of trip on.
    pub fn trips_only(self) -> Option<Kinds> {
        self.has_trips().then(|| Kinds {
            on: [self.on[0], self.on[1], false],
        })
    }

    pub fn has_collections(self) -> bool {
        self.contains(Kind::Collections)
    }

    /// The one kind of trip to ask the archive for, or `None` for both, or
    /// when no trips are asked for at all — see [`Self::has_trips`].
    pub fn trip_kind(self) -> Option<TripKind> {
        match (self.contains(Kind::Recorded), self.contains(Kind::Planned)) {
            (true, false) => Some(TripKind::Recorded),
            (false, true) => Some(TripKind::Planned),
            _ => None,
        }
    }

    /// The `kind` parameter: the kinds on, comma-separated.
    pub fn param(self) -> String {
        self.iter().map(Kind::as_str).collect::<Vec<_>>().join(",")
    }

    /// The inverse of [`Self::param`]. Unknown names are left out, and a
    /// value naming none — blank, or nonsense — is the default. A link from
    /// before US-90 names one kind, and opens with that kind alone.
    pub fn from_param(value: &str) -> Self {
        let mut on = [false; 3];
        for name in value.split(',') {
            if let Some(kind) = Kind::ALL.into_iter().find(|kind| kind.as_str() == name) {
                on[kind.index()] = true;
            }
        }
        if on.contains(&true) {
            Self { on }
        } else {
            Self::default()
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(on: &[Kind]) -> Kinds {
        on.iter()
            .skip(1)
            .fold(Kinds::only(on[0]), |kinds, kind| kinds.toggled(*kind))
    }

    #[test]
    fn us90_recorded_trips_alone_are_the_default() {
        let default = Kinds::default();

        assert_eq!(default.iter().collect::<Vec<_>>(), [Kind::Recorded]);
        assert_eq!(default.param(), "recorded");
    }

    #[test]
    fn us90_kinds_toggle_on_and_off_in_any_combination() {
        let all = kinds(&[Kind::Recorded, Kind::Planned, Kind::Collections]);

        assert_eq!(all.iter().collect::<Vec<_>>(), Kind::ALL);
        let without_planned = all.toggled(Kind::Planned);
        assert_eq!(
            without_planned.iter().collect::<Vec<_>>(),
            [Kind::Recorded, Kind::Collections]
        );
        assert_eq!(without_planned.toggled(Kind::Planned), all);
    }

    #[test]
    fn us90_the_last_kind_on_cannot_be_switched_off() {
        let collections = Kinds::only(Kind::Collections);

        assert!(collections.is_last(Kind::Collections));
        assert!(!collections.is_last(Kind::Recorded));
        assert_eq!(collections.toggled(Kind::Collections), collections);
    }

    #[test]
    fn us90_the_kinds_on_travel_as_one_parameter_and_back() {
        let some = kinds(&[Kind::Planned, Kind::Collections]);

        assert_eq!(some.param(), "planned,collections");
        assert_eq!(Kinds::from_param(&some.param()), some);
    }

    #[test]
    fn us90_a_link_naming_one_kind_opens_with_that_kind_alone() {
        assert_eq!(Kinds::from_param("planned"), Kinds::only(Kind::Planned));
        assert_eq!(
            Kinds::from_param("collections"),
            Kinds::only(Kind::Collections)
        );
    }

    #[test]
    fn us90_unknown_kinds_are_left_out_and_none_is_the_default() {
        assert_eq!(
            Kinds::from_param("bogus,collections"),
            Kinds::only(Kind::Collections)
        );
        assert_eq!(Kinds::from_param("nonsense"), Kinds::default());
        assert_eq!(Kinds::from_param(""), Kinds::default());
    }

    #[test]
    fn us90_trips_are_asked_for_by_kind_only_when_one_kind_of_trip_is_on() {
        assert_eq!(Kinds::default().trip_kind(), Some(TripKind::Recorded));
        assert_eq!(
            kinds(&[Kind::Planned, Kind::Collections]).trip_kind(),
            Some(TripKind::Planned)
        );
        let both = kinds(&[Kind::Recorded, Kind::Planned]);
        assert_eq!(both.trip_kind(), None);
        assert!(both.has_trips());
        let collections = Kinds::only(Kind::Collections);
        assert!(!collections.has_trips());
        assert!(collections.has_collections());
    }

    #[test]
    fn us90_the_kinds_are_told_apart_by_line_style() {
        assert_eq!(Kind::Recorded.dash(), None, "solid");
        let planned = Kind::Planned.dash().expect("dashed");
        let collections = Kind::Collections.dash().expect("dotted");
        assert_ne!(planned, collections);
    }

    #[test]
    fn the_kinds_come_in_order_with_their_labels() {
        let labels: Vec<_> = Kind::ALL.iter().map(|kind| kind.label()).collect();

        assert_eq!(labels, ["Recorded", "Planned", "Collections"]);
        assert_eq!(Kind::Planned.trip_kind(), Some(TripKind::Planned));
        assert_eq!(Kind::Collections.trip_kind(), None);
        assert_eq!(Kind::of_trip(TripKind::Planned), Kind::Planned);
    }

    #[test]
    fn us90_the_kinds_of_trip_on_leave_collections_out() {
        let all = Kinds::default()
            .toggled(Kind::Planned)
            .toggled(Kind::Collections);

        assert_eq!(
            all.trips_only(),
            Some(Kinds::default().toggled(Kind::Planned)),
            "toggling collections leaves what the trips are read for alone"
        );
        assert_eq!(
            Kinds::only(Kind::Planned).trips_only(),
            Some(Kinds::only(Kind::Planned))
        );
        assert_eq!(Kinds::only(Kind::Collections).trips_only(), None);
    }
}
