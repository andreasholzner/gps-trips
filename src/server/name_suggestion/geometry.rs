//! Distances in metres between a track and the places around it.
//!
//! Everything is projected onto a flat plane around the track first — an
//! equirectangular projection centred on it, accurate to a few percent
//! across the few hundred kilometres a trip spans, far finer than the
//! reaches it is compared against.

use geo::{Coord, EuclideanDistance, Line, MapCoords, MultiPolygon, Point};
use rstar::{primitives::GeomWithData, RTree, AABB};

use crate::config::name_suggestion::TURNING_CLIMB_FACTOR;
use crate::server::places::Shape;

const METRES_PER_DEGREE_LAT: f64 = 110_574.0;
const METRES_PER_DEGREE_LON_AT_EQUATOR: f64 = 111_320.0;

/// The flat plane a track and its places are measured on.
pub struct Plane {
    origin: Coord,
    metres_per_degree_lon: f64,
}

impl Plane {
    /// The plane centred on the bounding box of `coords` (longitude/
    /// latitude), which must not be empty.
    pub fn around(coords: &[Coord]) -> Self {
        let (min, max) = bounds(coords);
        let origin = (min + max) / 2.0;
        Self {
            origin,
            metres_per_degree_lon: METRES_PER_DEGREE_LON_AT_EQUATOR * origin.y.to_radians().cos(),
        }
    }

    pub fn project(&self, c: Coord) -> Coord {
        Coord {
            x: (c.x - self.origin.x) * self.metres_per_degree_lon,
            y: (c.y - self.origin.y) * METRES_PER_DEGREE_LAT,
        }
    }

    pub fn project_shape(&self, shape: &Shape) -> Flat {
        match shape {
            Shape::Point(c) => Flat::Point(self.project(*c)),
            Shape::Area(area) => Flat::Area(area.map_coords(|c| self.project(c))),
        }
    }
}

/// A place's shape on the plane.
#[derive(Debug, Clone)]
pub enum Flat {
    Point(Coord),
    Area(MultiPolygon),
}

impl Flat {
    /// How far `point` is from this shape: from its outline, or 0 inside.
    pub fn distance_to(&self, point: Coord) -> f64 {
        match self {
            Self::Point(c) => distance(*c, point),
            Self::Area(area) => area
                .0
                .iter()
                .map(|polygon| Point(point).euclidean_distance(polygon))
                .fold(f64::INFINITY, f64::min),
        }
    }

    /// How far apart this shape and `other` are, outline to outline.
    pub fn distance_to_shape(&self, other: &Flat) -> f64 {
        match (self, other) {
            (Self::Point(c), shape) | (shape, Self::Point(c)) => shape.distance_to(*c),
            (Self::Area(a), Self::Area(b)) => {
                a.0.iter()
                    .flat_map(|pa| b.0.iter().map(move |pb| pa.euclidean_distance(pb)))
                    .fold(f64::INFINITY, f64::min)
            }
        }
    }
}

/// The south-west and north-east corners of `coords`, which must not be
/// empty.
pub fn bounds(coords: &[Coord]) -> (Coord, Coord) {
    coords[1..]
        .iter()
        .fold((coords[0], coords[0]), |(min, max), c| {
            (
                Coord {
                    x: min.x.min(c.x),
                    y: min.y.min(c.y),
                },
                Coord {
                    x: max.x.max(c.x),
                    y: max.y.max(c.y),
                },
            )
        })
}

pub fn distance(a: Coord, b: Coord) -> f64 {
    (a - b).x.hypot((a - b).y)
}

/// Where a track passes a place: how close, and at which of its points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Passing {
    pub distance_m: f64,
    /// The index of the track point nearest the place, for its order along
    /// the track.
    pub index: usize,
}

/// A track on the plane, its segments indexed to find what passes near.
pub struct Track {
    pub points: Vec<Coord>,
    /// Each point's elevation, where the track has one.
    elevations: Vec<Option<f64>>,
    segments: RTree<GeomWithData<Line, usize>>,
}

impl Track {
    /// The track through `points` at `elevations`, which must not be empty.
    pub fn new(points: Vec<Coord>, elevations: Vec<Option<f64>>) -> Self {
        let segments = if points.len() == 1 {
            vec![GeomWithData::new(Line::new(points[0], points[0]), 0)]
        } else {
            points
                .windows(2)
                .enumerate()
                .map(|(i, pair)| GeomWithData::new(Line::new(pair[0], pair[1]), i))
                .collect()
        };
        Self {
            points,
            elevations,
            segments: RTree::bulk_load(segments),
        }
    }

    pub fn start(&self) -> Coord {
        self.points[0]
    }

    pub fn end(&self) -> Coord {
        self.points[self.points.len() - 1]
    }

    /// Where the track passes `shape`, if it comes within `reach_m` of it.
    pub fn passing(&self, shape: &Flat, reach_m: f64) -> Option<Passing> {
        let (lo, hi) = match shape {
            Flat::Point(c) => (*c, *c),
            Flat::Area(area) => {
                let rect = geo::BoundingRect::bounding_rect(area)?;
                (rect.min(), rect.max())
            }
        };
        let envelope = AABB::from_corners(
            Point::new(lo.x - reach_m, lo.y - reach_m),
            Point::new(hi.x + reach_m, hi.y + reach_m),
        );
        self.segments
            .locate_in_envelope_intersecting(&envelope)
            .filter_map(|segment| {
                let line = segment.geom();
                let distance_m = match shape {
                    Flat::Point(c) => Point(*c).euclidean_distance(line),
                    Flat::Area(area) => area
                        .0
                        .iter()
                        .map(|polygon| line.euclidean_distance(polygon))
                        .fold(f64::INFINITY, f64::min),
                };
                let nearer_end = match shape {
                    Flat::Point(c) if distance(line.end, *c) < distance(line.start, *c) => 1,
                    _ => 0,
                };
                (distance_m <= reach_m).then_some(Passing {
                    distance_m,
                    index: (segment.data + nearer_end).min(self.points.len() - 1),
                })
            })
            .min_by(|a, b| a.distance_m.total_cmp(&b.distance_m))
    }

    /// The index of the point farthest from the start, a metre climbed
    /// above it counting [`TURNING_CLIMB_FACTOR`] metres out: where a round
    /// trip turned — on a hike, its summit.
    pub fn turning_point(&self) -> usize {
        let start = self.start();
        let start_ele = self.elevations.first().copied().flatten();
        let effort = |i: usize| {
            let climbed = match (start_ele, self.elevations.get(i).copied().flatten()) {
                (Some(from), Some(to)) => (to - from).max(0.0),
                _ => 0.0,
            };
            distance(self.points[i], start) + TURNING_CLIMB_FACTOR * climbed
        };
        (0..self.points.len())
            .max_by(|&a, &b| effort(a).total_cmp(&effort(b)))
            .unwrap_or(0)
    }
}
