//! What a trip page shows ticked on its map beside the trip when it opens:
//! the other end of a proposed link the owner came from (US-103). A segment
//! of its own route, `/trips/:id/showing/:item`, so the Proposed links page
//! can lead there and the plain trip URL stays as it is. Pure and
//! Dioxus-free, like `filters.rs`.

use std::{fmt, str::FromStr};

use trip_archive_types::LinkOther;

/// The item, as its segment names it: `trip-5`, `track-7`, `waypoint-9`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Showing(pub LinkOther);

impl fmt::Display for Showing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            LinkOther::Trip(id) => write!(f, "trip-{id}"),
            LinkOther::CollectionTrack(id) => write!(f, "track-{id}"),
            LinkOther::Waypoint(id) => write!(f, "waypoint-{id}"),
        }
    }
}

impl FromStr for Showing {
    type Err = String;

    fn from_str(segment: &str) -> Result<Self, Self::Err> {
        let unknown = || format!("nothing to show called {segment:?}");
        let (kind, id) = segment.split_once('-').ok_or_else(unknown)?;
        let id = id.parse().map_err(|_| unknown())?;
        match kind {
            "trip" => Ok(Self(LinkOther::Trip(id))),
            "track" => Ok(Self(LinkOther::CollectionTrack(id))),
            "waypoint" => Ok(Self(LinkOther::Waypoint(id))),
            _ => Err(unknown()),
        }
    }
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us103_what_is_shown_round_trips_through_its_segment() {
        for other in [
            LinkOther::Trip(5),
            LinkOther::CollectionTrack(7),
            LinkOther::Waypoint(9),
        ] {
            let showing = Showing(other);
            assert_eq!(showing.to_string().parse::<Showing>(), Ok(showing));
        }
        assert_eq!(
            Showing(LinkOther::CollectionTrack(7)).to_string(),
            "track-7"
        );
    }

    #[test]
    fn us103_a_segment_it_does_not_know_shows_nothing() {
        for segment in ["trip-x", "photo-3", "", "trip5"] {
            assert!(segment.parse::<Showing>().is_err(), "{segment}");
        }
    }
}
