//! What a place weighs, and the places the sources name twice counted once.

use crate::config::name_suggestion::{
    base_weight, AREA_WEIGHT_PER_KM, DUPLICATE_REACH_M, ELEVATION_PER_WEIGHT_M,
    PROMINENCE_PER_WEIGHT_M,
};
use crate::server::places::{Place, PlaceKind};

use super::geometry::{Flat, Plane};

/// A place and its shape on the plane.
#[derive(Debug, Clone)]
pub struct Located {
    pub place: Place,
    pub flat: Flat,
}

impl Located {
    pub fn new(place: Place, plane: &Plane) -> Self {
        let flat = plane.project_shape(&place.shape);
        Self { place, flat }
    }

    /// How important the place is, by what the sources say about it: a
    /// summit's prominence (or else its height), a lake's area, a
    /// settlement's kind.
    pub fn weight(&self) -> f64 {
        let place = &self.place;
        let size = match place.kind {
            PlaceKind::Summit => place
                .prominence_m
                .map(|m| m / PROMINENCE_PER_WEIGHT_M)
                .or(place.ele_m.map(|m| m / ELEVATION_PER_WEIGHT_M)),
            PlaceKind::Lake | PlaceKind::Bay | PlaceKind::Glacier => place
                .area_m2
                .map(|m2| (m2 / 1_000_000.0).sqrt() * AREA_WEIGHT_PER_KM),
            _ => None,
        };
        base_weight(place.kind) + size.unwrap_or(0.0).max(0.0)
    }
}

/// `places` with every place the sources name twice counted once: two of a
/// kind within [`DUPLICATE_REACH_M`] of each other, from different sources
/// or under the same name. The one kept is the named one over one known
/// only by its height, and it takes on what the other knows about its size.
pub fn dedupe(places: Vec<Located>) -> Vec<Located> {
    let mut kept: Vec<Located> = Vec::with_capacity(places.len());
    for candidate in places {
        let twin = kept.iter_mut().find(|other| {
            other.place.kind == candidate.place.kind
                && (other.place.source != candidate.place.source
                    || other.place.name.eq_ignore_ascii_case(&candidate.place.name))
                && other.flat.distance_to_shape(&candidate.flat) <= DUPLICATE_REACH_M
        });
        match twin {
            Some(twin) => merge(twin, candidate),
            None => kept.push(candidate),
        }
    }
    kept
}

fn merge(kept: &mut Located, other: Located) {
    let other = if kept.place.is_height_only() && !other.place.is_height_only() {
        std::mem::replace(kept, other)
    } else {
        other
    };
    let place = &mut kept.place;
    place.ele_m = place.ele_m.or(other.place.ele_m);
    place.prominence_m = place.prominence_m.or(other.place.prominence_m);
    place.area_m2 = place.area_m2.or(other.place.area_m2);
    place.population = place.population.or(other.place.population);
}
