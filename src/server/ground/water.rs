//! Water as the ground data stores it: polygons cut into pieces small
//! enough that telling whether a point is on one is quick — the sea's
//! polygons run to hundreds of thousands of points.
//!
//! A piece is cut ring by ring against one half-plane at a time
//! (Sutherland–Hodgman). Cutting a concave ring that way can leave it with
//! edges doubling back along the cut; the even-odd test below counts those
//! twice, so they change nothing.

use geo::{Coord, LineString, Polygon};

/// Pieces of `polygon` with at most `max_points` points each, together
/// covering what it covers.
pub fn split(polygon: Polygon, max_points: usize) -> Vec<Polygon> {
    let points =
        polygon.exterior().0.len() + polygon.interiors().iter().map(|r| r.0.len()).sum::<usize>();
    if points <= max_points {
        return vec![polygon];
    }
    let Some((min, max)) = bounds(polygon.exterior()) else {
        return Vec::new();
    };
    let (axis, middle) = if max.x - min.x >= max.y - min.y {
        (Axis::X, (min.x + max.x) / 2.0)
    } else {
        (Axis::Y, (min.y + max.y) / 2.0)
    };
    [Side::Below, Side::Above]
        .into_iter()
        .filter_map(|side| clip(&polygon, axis, middle, side))
        .flat_map(|half| split(half, max_points))
        .collect()
}

/// Whether `point` is on `piece`: inside an odd number of its rings.
pub fn contains(piece: &Polygon, point: Coord) -> bool {
    std::iter::once(piece.exterior())
        .chain(piece.interiors())
        .filter(|ring| crosses_odd(ring, point))
        .count()
        % 2
        == 1
}

/// Whether a ray from `point` eastwards crosses `ring` an odd number of
/// times.
fn crosses_odd(ring: &LineString, point: Coord) -> bool {
    let mut inside = false;
    for edge in ring.0.windows(2) {
        let (a, b) = (edge[0], edge[1]);
        if (a.y > point.y) != (b.y > point.y) {
            let x = a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if point.x < x {
                inside = !inside;
            }
        }
    }
    inside
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
}

#[derive(Clone, Copy)]
enum Side {
    Below,
    Above,
}

/// The part of `polygon` on `side` of the line `axis = at`; `None` if
/// nothing of it is.
fn clip(polygon: &Polygon, axis: Axis, at: f64, side: Side) -> Option<Polygon> {
    let exterior = clip_ring(polygon.exterior(), axis, at, side)?;
    let interiors = polygon
        .interiors()
        .iter()
        .filter_map(|ring| clip_ring(ring, axis, at, side))
        .collect();
    Some(Polygon::new(exterior, interiors))
}

fn clip_ring(ring: &LineString, axis: Axis, at: f64, side: Side) -> Option<LineString> {
    let value = |c: Coord| match axis {
        Axis::X => c.x,
        Axis::Y => c.y,
    };
    let keeps = |c: Coord| match side {
        Side::Below => value(c) <= at,
        Side::Above => value(c) >= at,
    };
    let crossing = |a: Coord, b: Coord| {
        let t = (at - value(a)) / (value(b) - value(a));
        let c = a + (b - a) * t;
        match axis {
            Axis::X => Coord { x: at, y: c.y },
            Axis::Y => Coord { x: c.x, y: at },
        }
    };
    let mut out = Vec::new();
    for edge in ring.0.windows(2) {
        let (a, b) = (edge[0], edge[1]);
        match (keeps(a), keeps(b)) {
            (true, true) => out.push(b),
            (true, false) => out.push(crossing(a, b)),
            (false, true) => {
                out.push(crossing(a, b));
                out.push(b);
            }
            (false, false) => {}
        }
    }
    if out.len() < 3 {
        return None;
    }
    out.push(out[0]);
    Some(LineString(out))
}

fn bounds(ring: &LineString) -> Option<(Coord, Coord)> {
    let first = *ring.0.first()?;
    Some(ring.0.iter().fold((first, first), |(min, max), c| {
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
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::polygon;

    /// A C shape: a 10 × 10 square with a 6 wide notch cut into its east
    /// side, round an island, with many points along its outline.
    fn c_shape_with_island() -> Polygon {
        let corners = [
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 2.0),
            (4.0, 2.0),
            (4.0, 8.0),
            (10.0, 8.0),
            (10.0, 10.0),
            (0.0, 10.0),
            (0.0, 0.0),
        ];
        let mut ring = Vec::new();
        for pair in corners.windows(2) {
            let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
            for i in 0..50 {
                let t = i as f64 / 50.0;
                ring.push(Coord {
                    x: x0 + (x1 - x0) * t,
                    y: y0 + (y1 - y0) * t,
                });
            }
        }
        ring.push(ring[0]);
        let island = LineString::from(vec![
            (1.0, 4.0),
            (2.0, 4.0),
            (2.0, 6.0),
            (1.0, 6.0),
            (1.0, 4.0),
        ]);
        Polygon::new(LineString(ring), vec![island])
    }

    #[test]
    fn us76_a_point_is_on_water_inside_the_outline_and_off_it_on_an_island() {
        let water = c_shape_with_island();
        assert!(contains(&water, Coord { x: 3.0, y: 1.0 }));
        assert!(!contains(&water, Coord { x: 7.0, y: 5.0 }), "in the notch");
        assert!(!contains(&water, Coord { x: 1.5, y: 5.0 }), "on the island");
        assert!(!contains(&water, Coord { x: 11.0, y: 5.0 }));
    }

    #[test]
    fn us76_the_pieces_are_small_and_cover_what_the_whole_covers() {
        let water = c_shape_with_island();

        let pieces = split(water.clone(), 60);

        assert!(pieces.len() > 4, "{}", pieces.len());
        for piece in &pieces {
            let points = piece.exterior().0.len()
                + piece.interiors().iter().map(|r| r.0.len()).sum::<usize>();
            assert!(points <= 60, "{points}");
        }
        // On a grid of points, avoiding the cut lines, the pieces say what
        // the whole says.
        for i in 0..40 {
            for j in 0..40 {
                let point = Coord {
                    x: -0.4 + i as f64 * 0.27,
                    y: -0.4 + j as f64 * 0.27,
                };
                let on_a_piece = pieces.iter().filter(|p| contains(p, point)).count();
                assert_eq!(on_a_piece == 1, contains(&water, point), "{point:?}");
                assert!(on_a_piece <= 1, "{point:?}");
            }
        }
    }

    #[test]
    fn us76_a_small_polygon_is_one_piece() {
        let small = polygon![(x: 0.0, y: 0.0), (x: 1.0, y: 0.0), (x: 0.0, y: 1.0)];
        assert_eq!(split(small.clone(), 100), vec![small]);
    }
}
