//! The boxes to look things up in along a track: around each stretch of it
//! rather than around the whole, so a long trip does not fetch everything
//! in the rectangle it spans — the places of US-74, the ground of US-76.

use geo::{Coord, Rect};

/// Track points per stretch.
const STRETCH_POINTS: usize = 200;

/// Boxes (longitude/latitude) covering everything within `reach_m` of the
/// track through `coords`.
pub fn around(coords: &[Coord], reach_m: f64) -> Vec<Rect> {
    if coords.is_empty() {
        return Vec::new();
    }
    // A degree of longitude is shortest where the track is farthest from
    // the equator, so the margin is measured there.
    let widest = coords.iter().map(|c| c.y.abs()).fold(0.0, f64::max);
    let margin = Coord {
        x: reach_m / (111_320.0 * widest.to_radians().cos().max(0.01)),
        y: reach_m / 110_574.0,
    };
    (0..coords.len())
        .step_by(STRETCH_POINTS)
        .map(|from| {
            // Each stretch ends where the next one starts, so no segment
            // falls between two boxes.
            let to = (from + STRETCH_POINTS + 1).min(coords.len());
            let (min, max) = bounds(&coords[from..to]);
            Rect::new(min - margin, max + margin)
        })
        .collect()
}

fn bounds(coords: &[Coord]) -> (Coord, Coord) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use geo::Contains;

    /// `x` metres east and `y` north of 8° E, 60° N.
    fn at(x: f64, y: f64) -> Coord {
        Coord {
            x: 8.0 + x / (111_320.0 * 60f64.to_radians().cos()),
            y: 60.0 + y / 110_574.0,
        }
    }

    #[test]
    fn us74_boxes_go_stretch_by_stretch_as_far_as_the_reach() {
        // East 30 km, then north 30 km, a point every 50 m.
        let coords: Vec<Coord> = (0..=600)
            .map(|i| at(f64::from(i) * 50.0, 0.0))
            .chain((1..=600).map(|i| at(30_000.0, f64::from(i) * 50.0)))
            .collect();

        let boxes = around(&coords, 5_000.0);

        // Several stretches, not one box over the whole corner.
        assert!(boxes.len() > 1, "{boxes:?}");
        assert!(!boxes.iter().any(|b| b.contains(&at(15_000.0, 15_000.0))));
        for c in [
            at(0.0, -4_999.0),
            at(34_999.0, 30_000.0),
            at(15_000.0, 4_999.0),
        ] {
            assert!(boxes.iter().any(|b| b.contains(&c)), "{c:?} not covered");
        }
        assert!(around(&[], 5_000.0).is_empty());
    }
}
