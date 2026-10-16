//! Lines on a flat projection, and whether a point lies near one — shared by
//! matching a file against a collection (ADR-0029) and a trip against a plan
//! (ADR-0030). Distances are metres on a projection around one origin: over
//! a trip's extent the earth is flat enough for tolerances of tens of metres.

use std::collections::HashMap;

const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// A `(lat, lon)` position.
pub type Position = (f64, f64);

/// Metres east and north of the projection's origin.
pub type Flat = (f64, f64);

#[derive(Clone, Copy)]
pub struct Projection {
    lat0_cos: f64,
}

impl Projection {
    pub fn around((lat, _): Position) -> Self {
        Self {
            lat0_cos: lat.to_radians().cos(),
        }
    }

    /// Metres east and north.
    pub fn apply(self, (lat, lon): Position) -> Flat {
        (
            lon.to_radians() * self.lat0_cos * EARTH_RADIUS_M,
            lat.to_radians() * EARTH_RADIUS_M,
        )
    }
}

/// Metres between two flat points.
pub fn distance(a: Flat, b: Flat) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

type Cell = (i64, i64);

fn cell_of((x, y): Flat, cell: f64) -> Cell {
    ((x / cell).floor() as i64, (y / cell).floor() as i64)
}

/// A line's segments filed in a grid of `tolerance`-sized cells, each in the
/// cells it passes through, so a point looks only at the segments in its own
/// and the neighbouring cells: the closest point of a segment within the
/// tolerance lies in one of them. A line of one point is a place.
pub struct SegmentGrid {
    segments: Vec<(Flat, Flat)>,
    grid: HashMap<Cell, Vec<usize>>,
    cell: f64,
    tolerance: f64,
}

impl SegmentGrid {
    pub fn new(line: &[Flat], tolerance: f64) -> Self {
        let cell = tolerance.max(1.0);
        let segments: Vec<(Flat, Flat)> = if line.len() == 1 {
            vec![(line[0], line[0])]
        } else {
            line.windows(2).map(|pair| (pair[0], pair[1])).collect()
        };
        let mut grid: HashMap<Cell, Vec<usize>> = HashMap::new();
        for (index, &(start, end)) in segments.iter().enumerate() {
            for filed in segment_cells(start, end, cell) {
                grid.entry(filed).or_default().push(index);
            }
        }
        Self {
            segments,
            grid,
            cell,
            tolerance,
        }
    }

    /// Whether `point` lies within the tolerance of the line.
    pub fn reaches(&self, point: Flat) -> bool {
        let (cx, cy) = cell_of(point, self.cell);
        (cx - 1..=cx + 1).any(|x| {
            (cy - 1..=cy + 1).any(|y| {
                self.grid.get(&(x, y)).is_some_and(|indices| {
                    indices.iter().any(|&index| {
                        let (start, end) = self.segments[index];
                        to_segment(point, start, end) <= self.tolerance
                    })
                })
            })
        })
    }
}

/// The cells a segment passes through, walked from its start to its end one
/// cell boundary at a time (Amanatides and Woo) — as many as it crosses, not
/// the whole rectangle around it.
fn segment_cells(start: Flat, end: Flat, cell: f64) -> Vec<Cell> {
    let (mut x, mut y) = cell_of(start, cell);
    let last = cell_of(end, cell);
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    // Per axis: which way the walk steps, how far along the segment (0 to 1)
    // the next boundary is, and how far apart the boundaries are.
    let axis = |d: f64, from: f64, at: i64| -> (i64, f64, f64) {
        if d == 0.0 {
            return (0, f64::INFINITY, f64::INFINITY);
        }
        let step = if d > 0.0 { 1 } else { -1 };
        let boundary = (at + i64::from(d > 0.0)) as f64 * cell;
        (step, (boundary - from) / d, cell / d.abs())
    };
    let (step_x, mut next_x, delta_x) = axis(dx, start.0, x);
    let (step_y, mut next_y, delta_y) = axis(dy, start.1, y);

    // The walk ends after as many steps as there are boundaries between the
    // two cells, whatever rounding does to the comparisons on the way.
    let steps = (last.0 - x).abs() + (last.1 - y).abs();
    let mut cells = Vec::with_capacity(steps as usize + 2);
    cells.push((x, y));
    for _ in 0..steps {
        // Through a corner: both cells beside it are touched too, and a point
        // near the corner may look only that far.
        if (next_x - next_y).abs() <= 1e-9 {
            cells.push((x + step_x, y));
            cells.push((x, y + step_y));
        }
        if next_x < next_y {
            x += step_x;
            next_x += delta_x;
        } else {
            y += step_y;
            next_y += delta_y;
        }
        cells.push((x, y));
    }
    if cells.last() != Some(&last) {
        cells.push(last);
    }
    cells
}

fn to_segment(p: Flat, a: Flat, b: Flat) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length_sq = dx * dx + dy * dy;
    let t = if length_sq == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length_sq).clamp(0.0, 1.0)
    };
    distance(p, (a.0 + t * dx, a.1 + t * dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us88_a_long_segment_is_filed_in_the_cells_along_it_only() {
        // A sparse route's 20 km diagonal leg, at a 30 m tolerance: along it
        // are some 1,300 cells, around it some 440,000.
        let (start, end) = ((0.0, 0.0), (20_000.0, 14_000.0));

        let cells = segment_cells(start, end, 30.0);

        assert!(cells.len() < 1_500, "{}", cells.len());
        for i in 0..=10_000 {
            let t = f64::from(i) / 10_000.0;
            let on = (
                start.0 + t * (end.0 - start.0),
                start.1 + t * (end.1 - start.1),
            );
            assert!(
                cells.contains(&cell_of(on, 30.0)),
                "{on:?} is on the segment"
            );
        }
    }

    #[test]
    fn us103_a_point_beside_a_long_segment_reaches_it_with_no_vertex_near() {
        let grid = SegmentGrid::new(&[(0.0, 0.0), (10_000.0, 0.0)], 50.0);

        assert!(grid.reaches((5_000.0, 40.0)));
        assert!(!grid.reaches((5_000.0, 60.0)));
    }
}
