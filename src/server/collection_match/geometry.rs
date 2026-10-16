//! Whether two lines, or two places, lie within a distance of each other
//! (ADR-0029). Distances are worked out on a flat projection around the
//! first line's start: tolerances are metres, over which the earth is flat.

use crate::server::line_grid::{distance, Projection, SegmentGrid};

pub use crate::server::line_grid::Position;

/// Metres between two positions, flat — exact enough at tolerance range.
pub fn distance_m(a: Position, b: Position) -> f64 {
    let projection = Projection::around(a);
    distance(projection.apply(a), projection.apply(b))
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
    let (near_a, near_b) = (
        SegmentGrid::new(&a, tolerance_m),
        SegmentGrid::new(&b, tolerance_m),
    );
    a.iter().all(|&p| near_b.reaches(p)) && b.iter().all(|&p| near_a.reaches(p))
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
