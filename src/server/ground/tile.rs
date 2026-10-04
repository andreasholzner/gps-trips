//! The grid the ways are stored on: Web Mercator cells at zoom 20 — 13 m
//! across in the far north, 26 m in the Alps, about what a GPS strays by — in
//! tiles of 32 × 32 cells, a zoom-15 tile each. A tile holds one bitmap per
//! kind of way, stored compressed: most cells hold none.

use std::f64::consts::PI;
use std::io::{Read, Write};

use flate2::{read::DeflateDecoder, write::DeflateEncoder, Compression};
use geo::Coord;

/// Cells along each axis of the world.
const CELLS: f64 = (1u64 << 20) as f64;
/// Cells along each side of a tile.
const TILE_CELLS: i64 = 32;
const WORDS: usize = (TILE_CELLS * TILE_CELLS / 64) as usize;

/// The kinds of way the ground data tells apart (US-76).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    Road = 0,
    /// A track, a cycleway or a gravel path.
    Good = 1,
    /// A small path.
    Small = 2,
}

impl Way {
    pub const ALL: [Self; 3] = [Self::Road, Self::Good, Self::Small];
}

/// A cell of the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell {
    pub x: i64,
    pub y: i64,
}

impl Cell {
    /// The cell `at` (longitude/latitude) lies in.
    pub fn at(at: Coord) -> Self {
        let (x, y) = fractional(at);
        Self {
            x: x.floor() as i64,
            y: y.floor() as i64,
        }
    }

    /// The column and row of the tile the cell is in.
    pub fn tile_xy(self) -> (i64, i64) {
        (self.x.div_euclid(TILE_CELLS), self.y.div_euclid(TILE_CELLS))
    }

    /// The id of the tile the cell is in.
    pub fn tile(self) -> i64 {
        let (x, y) = self.tile_xy();
        (x << 16) | y
    }

    fn bit(self) -> usize {
        (self.y.rem_euclid(TILE_CELLS) * TILE_CELLS + self.x.rem_euclid(TILE_CELLS)) as usize
    }

    /// The cell and the eight around it, the cell first.
    pub fn with_neighbours(self) -> impl Iterator<Item = Cell> {
        [
            (0, 0),
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ]
        .into_iter()
        .map(move |(dx, dy)| Cell {
            x: self.x + dx,
            y: self.y + dy,
        })
    }
}

/// Web Mercator coordinates of `at`, in cells.
fn fractional(at: Coord) -> (f64, f64) {
    let lat = at.y.clamp(-85.0, 85.0).to_radians();
    let x = (at.x + 180.0) / 360.0 * CELLS;
    let y = (1.0 - lat.tan().asinh() / PI) / 2.0 * CELLS;
    (x, y)
}

/// The cells a straight segment from `a` to `b` crosses.
pub fn cells_along(a: Coord, b: Coord) -> impl Iterator<Item = Cell> {
    let (x0, y0) = fractional(a);
    let (x1, y1) = fractional(b);
    // Half a cell at a time, so no cell it crosses is stepped over.
    let steps = ((x1 - x0).abs().max((y1 - y0).abs()) * 2.0).ceil().max(1.0) as usize;
    (0..=steps).map(move |k| {
        let t = k as f64 / steps as f64;
        Cell {
            x: (x0 + (x1 - x0) * t).floor() as i64,
            y: (y0 + (y1 - y0) * t).floor() as i64,
        }
    })
}

/// One tile's ways: a bitmap per kind.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tile([[u64; WORDS]; 3]);

impl Tile {
    pub fn set(&mut self, cell: Cell, way: Way) {
        let bit = cell.bit();
        self.0[way as usize][bit / 64] |= 1 << (bit % 64);
    }

    pub fn has(&self, cell: Cell, way: Way) -> bool {
        let bit = cell.bit();
        self.0[way as usize][bit / 64] & (1 << (bit % 64)) != 0
    }

    /// Adds `other`'s ways: the same tile from two regions' extracts.
    pub fn merge(&mut self, other: &Tile) {
        for (mine, theirs) in self.0.iter_mut().zip(&other.0) {
            for (a, b) in mine.iter_mut().zip(theirs) {
                *a |= b;
            }
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
        for word in self.0.iter().flatten() {
            encoder
                .write_all(&word.to_le_bytes())
                .expect("writing to memory");
        }
        encoder.finish().expect("writing to memory")
    }

    /// The tile `bytes` hold; `None` for bytes that are not one.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut raw = Vec::with_capacity(3 * WORDS * 8);
        DeflateDecoder::new(bytes).read_to_end(&mut raw).ok()?;
        if raw.len() != 3 * WORDS * 8 {
            return None;
        }
        let mut tile = Tile::default();
        for (word, chunk) in tile.0.iter_mut().flatten().zip(raw.as_chunks::<8>().0) {
            *word = u64::from_le_bytes(*chunk);
        }
        Some(tile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TROMSO: Coord = Coord { x: 18.95, y: 69.65 };

    #[test]
    fn us76_a_cell_is_about_thirteen_metres_in_the_far_north() {
        // 0.001° of latitude is 111 m: eight or nine cells at 69.6° N.
        let a = Cell::at(TROMSO);
        let b = Cell::at(Coord {
            x: TROMSO.x,
            y: TROMSO.y + 0.001,
        });
        assert!((a.y - b.y) >= 8 && (a.y - b.y) <= 9, "{a:?} {b:?}");
    }

    #[test]
    fn us76_a_segment_marks_every_cell_it_crosses_and_no_other() {
        let a = TROMSO;
        let b = Coord {
            x: TROMSO.x + 0.01,
            y: TROMSO.y + 0.002,
        };
        let cells: Vec<Cell> = cells_along(a, b).collect();
        let (first, last) = (Cell::at(a), Cell::at(b));
        assert_eq!(cells.first(), Some(&first));
        assert_eq!(cells.last(), Some(&last));
        // Connected: each next cell touches the one before.
        for pair in cells.windows(2) {
            assert!((pair[0].x - pair[1].x).abs() <= 1 && (pair[0].y - pair[1].y).abs() <= 1);
        }
    }

    #[test]
    fn us76_a_tile_keeps_its_ways_through_storage() {
        let cell = Cell::at(TROMSO);
        let mut tile = Tile::default();
        tile.set(cell, Way::Small);

        let read = Tile::decode(&tile.encode()).expect("a tile");

        assert!(read.has(cell, Way::Small));
        assert!(!read.has(cell, Way::Road));
        let beside = Cell {
            x: cell.x + 1,
            ..cell
        };
        assert!(!read.has(beside, Way::Small));
        // Mostly empty, so small.
        assert!(tile.encode().len() < 40, "{}", tile.encode().len());
        assert_eq!(Tile::decode(b"not a tile"), None);
    }

    #[test]
    fn us76_two_extracts_ways_in_one_tile_add_up() {
        let cell = Cell::at(TROMSO);
        let (mut a, mut b) = (Tile::default(), Tile::default());
        a.set(cell, Way::Road);
        b.set(cell, Way::Good);

        a.merge(&b);

        assert!(a.has(cell, Way::Road) && a.has(cell, Way::Good));
    }

    #[test]
    fn us76_neighbours_across_a_tile_edge_are_in_the_next_tile() {
        let edge = Cell { x: 31, y: 0 };
        let tiles: Vec<i64> = edge.with_neighbours().map(Cell::tile).collect();
        assert_eq!(tiles[0], edge.tile());
        assert!(tiles.contains(&Cell { x: 32, y: 0 }.tile()));
        assert_ne!(Cell { x: 32, y: 0 }.tile(), edge.tile());
    }
}
