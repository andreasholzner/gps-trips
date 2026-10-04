//! The ground a track runs on (US-76, ADR-0027): the ways and the water an
//! activity type is suggested from, read offline from OpenStreetMap.
//!
//! Kept in a database of its own beside the place names, built the same
//! way (`places_build ground`) and as optional: without it no activity type
//! is suggested.

pub mod build;
mod db;
pub mod osm;
pub mod tile;
pub mod water;

pub use db::{GroundDb, GroundWriter};
pub use tile::{Cell, Tile, Way};

use std::collections::HashMap;

use geo::{Coord, HaversineDistance, Intersects, Point, Polygon, Rect};
use rstar::{primitives::GeomWithData, RTree};

use crate::server::activity_suggestion::Ground;

/// What one point of a track is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Water,
    /// A road, or a good path: a track, a cycleway, gravel.
    Road,
    /// A small path, or no way at all.
    OffRoad,
    /// Somewhere the data does not cover.
    Unknown,
}

/// What the ground data knows around one track.
pub struct GroundData {
    tiles: HashMap<i64, Tile>,
    water: RTree<GeomWithData<rstar::primitives::Rectangle<Point>, usize>>,
    pieces: Vec<Polygon>,
    coverage: Vec<Rect>,
}

impl GroundData {
    pub fn new(tiles: HashMap<i64, Tile>, pieces: Vec<Polygon>, coverage: Vec<Rect>) -> Self {
        let water = RTree::bulk_load(
            pieces
                .iter()
                .enumerate()
                .filter_map(|(i, piece)| {
                    let rect = geo::BoundingRect::bounding_rect(piece)?;
                    Some(GeomWithData::new(
                        rstar::primitives::Rectangle::from_corners(
                            Point(rect.min()),
                            Point(rect.max()),
                        ),
                        i,
                    ))
                })
                .collect(),
        );
        Self {
            tiles,
            water,
            pieces,
            coverage,
        }
    }

    /// What `at` is on: a way in its own cell, or else in a cell next to it
    /// — a road over a small path where both are — then water, then
    /// nothing at all.
    pub fn surface_at(&self, at: Coord) -> Surface {
        if !self.coverage.iter().any(|rect| rect.intersects(&at)) {
            return Surface::Unknown;
        }
        let cell = Cell::at(at);
        let ways_in = |cells: &[Cell]| -> Option<Surface> {
            let has = |way| {
                cells.iter().any(|c| {
                    self.tiles
                        .get(&c.tile())
                        .is_some_and(|tile| tile.has(*c, way))
                })
            };
            if has(Way::Road) || has(Way::Good) {
                Some(Surface::Road)
            } else if has(Way::Small) {
                Some(Surface::OffRoad)
            } else {
                None
            }
        };
        let around: Vec<Cell> = cell.with_neighbours().skip(1).collect();
        if let Some(surface) = ways_in(&[cell]).or_else(|| ways_in(&around)) {
            return surface;
        }
        let point = Point(at);
        let on_water = self
            .water
            .locate_all_at_point(&point)
            .any(|piece| water::contains(&self.pieces[piece.data], at));
        if on_water {
            Surface::Water
        } else {
            Surface::OffRoad
        }
    }

    /// What the track through `coords` runs on, as shares of its length:
    /// each stretch counts as what its first point is on.
    pub fn ground_of(&self, coords: &[Coord]) -> Ground {
        let mut metres = Ground::default();
        let mut total = 0.0;
        for pair in coords.windows(2) {
            let length = Point(pair[0]).haversine_distance(&Point(pair[1]));
            total += length;
            match self.surface_at(pair[0]) {
                Surface::Water => metres.water += length,
                Surface::Road => metres.road += length,
                Surface::OffRoad => metres.off_road += length,
                Surface::Unknown => metres.unknown += length,
            }
        }
        if total <= 0.0 {
            return Ground {
                unknown: 1.0,
                ..Ground::default()
            };
        }
        Ground {
            water: metres.water / total,
            road: metres.road / total,
            off_road: metres.off_road / total,
            unknown: metres.unknown / total,
        }
    }
}

#[cfg(test)]
mod tests;
