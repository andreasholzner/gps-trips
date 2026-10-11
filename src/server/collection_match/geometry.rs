//! Whether two lines, or two places, lie within a distance of each other
//! (ADR-0029). Distances are worked out on a flat projection around the
//! first line's start: tolerances are metres, over which the earth is flat.

use std::collections::HashMap;

const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// A `(lat, lon)` position.
pub type Position = (f64, f64);

/// Metres between two positions, flat — exact enough at tolerance range.
pub fn distance_m(a: Position, b: Position) -> f64 {
    let projection = Projection::around(a);
    let (ax, ay) = projection.apply(a);
    let (bx, by) = projection.apply(b);
    (ax - bx).hypot(ay - by)
}

/// Whether every point of each line lies within `tolerance_m` of the other
/// line. Lines without a point are never within anything.
pub fn lines_within(a: &[Position], b: &[Position], tolerance_m: f64) -> bool {
    let Some(&origin) = a.first() else {
        return false;
    };
    if b.is_empty() {
        return false;
    }
    let projection = Projection::around(origin);
    let a: Vec<_> = a.iter().map(|&p| projection.apply(p)).collect();
    let b: Vec<_> = b.iter().map(|&p| projection.apply(p)).collect();
    covered(&a, &b, tolerance_m) && covered(&b, &a, tolerance_m)
}

/// The centre of the rectangle around a line, for ranking matches.
pub fn centre(line: &[Position]) -> Option<Position> {
    let first = *line.first()?;
    let (min, max) = line.iter().fold((first, first), |(min, max), &(lat, lon)| {
        (
            (min.0.min(lat), min.1.min(lon)),
            (max.0.max(lat), max.1.max(lon)),
        )
    });
    Some(((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0))
}

#[derive(Clone, Copy)]
struct Projection {
    lat0_cos: f64,
}

impl Projection {
    fn around((lat, _): Position) -> Self {
        Self {
            lat0_cos: lat.to_radians().cos(),
        }
    }

    /// Metres east and north.
    fn apply(self, (lat, lon): Position) -> (f64, f64) {
        (
            lon.to_radians() * self.lat0_cos * EARTH_RADIUS_M,
            lat.to_radians() * EARTH_RADIUS_M,
        )
    }
}

type Flat = (f64, f64);

type Cell = (i64, i64);

fn cell_of((x, y): Flat, cell: f64) -> Cell {
    ((x / cell).floor() as i64, (y / cell).floor() as i64)
}

/// Whether every point lies within `tolerance` of `line`. The line's
/// segments are filed in a grid of `tolerance`-sized cells, each in the
/// cells it passes through, so each point looks only at the segments in its
/// own and the neighbouring cells: the closest point of a segment within
/// the tolerance lies in one of them.
fn covered(points: &[Flat], line: &[Flat], tolerance: f64) -> bool {
    let cell = tolerance.max(1.0);
    let key = |point: Flat| cell_of(point, cell);
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
    points.iter().all(|&point| {
        let (cx, cy) = key(point);
        (cx - 1..=cx + 1).any(|x| {
            (cy - 1..=cy + 1).any(|y| {
                grid.get(&(x, y)).is_some_and(|indices| {
                    indices.iter().any(|&index| {
                        let (start, end) = segments[index];
                        to_segment(point, start, end) <= tolerance
                    })
                })
            })
        })
    })
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
    (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// About 11 m of latitude.
    const STEP: f64 = 0.0001;

    fn line(offset_lat: f64) -> Vec<Position> {
        (0..50)
            .map(|i| (46.5 + offset_lat, 9.2 + i as f64 * 0.001))
            .collect()
    }

    #[test]
    fn us88_a_line_shifted_slightly_is_within_the_tolerance() {
        assert!(lines_within(&line(0.0), &line(STEP), 30.0));
    }

    #[test]
    fn us88_a_line_shifted_further_is_not() {
        assert!(!lines_within(&line(0.0), &line(5.0 * STEP), 30.0));
    }

    #[test]
    fn us88_a_line_resampled_along_the_same_way_is_within_the_tolerance() {
        // Fewer points on the same straight way: each lies on the other line.
        let sparse: Vec<Position> = line(0.0).into_iter().step_by(7).collect();

        assert!(lines_within(&line(0.0), &sparse, 30.0));
    }

    #[test]
    fn us88_a_line_that_goes_further_is_not_within_the_shorter_one() {
        let mut longer = line(0.0);
        longer.push((46.5, 9.3));

        assert!(!lines_within(&line(0.0), &longer, 30.0));
        assert!(!lines_within(&longer, &line(0.0), 30.0));
    }

    #[test]
    fn us88_single_points_compare_as_places() {
        assert!(lines_within(&[(46.5, 9.2)], &[(46.5 + STEP, 9.2)], 30.0));
        assert!(!lines_within(&[(46.5, 9.2)], &[], 30.0));
    }

    #[test]
    fn us88_a_long_segment_is_filed_in_the_cells_along_it_only() {
        // A sparse route's 20 km diagonal leg, at a 30 m tolerance: along it
        // are some 1,300 cells, around it some 440,000.
        let (start, end) = ((0.0, 0.0), (20_000.0, 14_000.0));

        let cells = segment_cells(start, end, 30.0);

        assert!(cells.len() < 1_500, "{}", cells.len());
        let key = |(x, y): Flat| ((x / 30.0_f64).floor() as i64, (y / 30.0_f64).floor() as i64);
        for i in 0..=10_000 {
            let t = f64::from(i) / 10_000.0;
            let on = (
                start.0 + t * (end.0 - start.0),
                start.1 + t * (end.1 - start.1),
            );
            assert!(cells.contains(&key(on)), "{on:?} is on the segment");
        }
    }

    #[test]
    fn us88_long_diagonal_lines_compare_as_short_ones_do() {
        let diagonal = |shift: f64| vec![(46.0 + shift, 9.0), (46.2 + shift, 9.2)];

        assert!(lines_within(&diagonal(0.0), &diagonal(NUDGE_DEG), 30.0));
        assert!(!lines_within(&diagonal(0.0), &diagonal(0.001), 30.0));
    }

    /// About 11 m of latitude.
    const NUDGE_DEG: f64 = 0.0001;

    #[test]
    fn us88_distances_are_metres() {
        let metres = distance_m((46.5, 9.2), (46.5 + 0.001, 9.2));

        assert!((metres - 111.2).abs() < 0.5, "{metres}");
    }
}
