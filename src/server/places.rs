//! The offline place names a trip's name is suggested from (US-74,
//! ADR-0027): what a place is, as the rules in `name_suggestion` see it, and
//! the read-only database the server finds them in.
//!
//! The database is a build artefact (`src/bin/places_build.rs`), kept apart
//! from the archive's own and optional at runtime: without it the suggestion
//! falls back to US-12's.

pub mod build;
mod db;
pub mod osm;
mod shape;

pub use db::{PlaceDb, PlaceWriter};
pub use shape::{decode_area, encode_area};

use geo::{Coord, MultiPolygon};

/// What kind of place a name belongs to (ADR-0018: a closed set). Decides
/// how far a place reaches and what it weighs (`config::name_suggestion`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaceKind {
    City,
    Town,
    Village,
    Hamlet,
    /// A farm or a single dwelling: never named, but a hut may be named
    /// after one.
    Farm,
    /// A named spot with nothing on it, likewise never named.
    Locality,
    Summit,
    Pass,
    Hut,
    Campsite,
    Lake,
    Bay,
    Glacier,
}

impl PlaceKind {
    pub const ALL: [Self; 13] = [
        Self::City,
        Self::Town,
        Self::Village,
        Self::Hamlet,
        Self::Farm,
        Self::Locality,
        Self::Summit,
        Self::Pass,
        Self::Hut,
        Self::Campsite,
        Self::Lake,
        Self::Bay,
        Self::Glacier,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::City => "city",
            Self::Town => "town",
            Self::Village => "village",
            Self::Hamlet => "hamlet",
            Self::Farm => "farm",
            Self::Locality => "locality",
            Self::Summit => "summit",
            Self::Pass => "pass",
            Self::Hut => "hut",
            Self::Campsite => "campsite",
            Self::Lake => "lake",
            Self::Bay => "bay",
            Self::Glacier => "glacier",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// Where a trip stopped for the night or set out from: a hut or a
    /// campsite right at an end names it.
    pub fn is_stop(self) -> bool {
        matches!(self, Self::Hut | Self::Campsite)
    }
}

/// Where a place's name comes from. A place two sources name counts once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Osm,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Osm => "osm",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "osm" => Some(Self::Osm),
            _ => None,
        }
    }
}

/// Where a place is, in longitude/latitude: a point, or an area — a lake,
/// a bay, a glacier — that counts by its outline rather than its centre.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Point(Coord),
    Area(MultiPolygon),
}

/// One named place, as the sources describe it.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// What the suggestion says: a summit known only by its height has that
    /// height as its name (`884`).
    pub name: String,
    pub kind: PlaceKind,
    pub source: Source,
    pub shape: Shape,
    pub ele_m: Option<f64>,
    pub prominence_m: Option<f64>,
    pub area_m2: Option<f64>,
    /// How many people live there, for a settlement that says.
    pub population: Option<u64>,
}

impl Place {
    /// A summit with no name but its height.
    pub fn is_height_only(&self) -> bool {
        self.name.bytes().all(|b| b.is_ascii_digit())
    }
}
