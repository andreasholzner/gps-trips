//! The ground data written, read back and asked what a point is on.

use geo::{Coord, LineString, Polygon, Rect};

use super::{GroundDb, GroundWriter, Surface, Way};

const ORIGIN: Coord = Coord { x: 8.0, y: 60.0 };

/// The longitude/latitude `x` metres east and `y` metres north of the
/// origin.
fn at(x: f64, y: f64) -> Coord {
    Coord {
        x: ORIGIN.x + x / (111_320.0 * ORIGIN.y.to_radians().cos()),
        y: ORIGIN.y + y / 110_574.0,
    }
}

fn square(x: f64, y: f64, side: f64) -> Polygon {
    let h = side / 2.0;
    Polygon::new(
        LineString(vec![
            at(x - h, y - h),
            at(x + h, y - h),
            at(x + h, y + h),
            at(x - h, y + h),
            at(x - h, y - h),
        ]),
        vec![],
    )
}

/// A road east along y = 0, a path east along y = 1000, a lake 1 km
/// across at (5000, 3000) with a bridge over it, all covered by a 20 km
/// box round the origin.
async fn a_world(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("ground.sqlite");
    let mut writer = GroundWriter::create(&path).await.expect("a new database");
    writer.add_way(Way::Road, &[at(0.0, 0.0), at(10_000.0, 0.0)]);
    writer.add_way(Way::Small, &[at(0.0, 1_000.0), at(10_000.0, 1_000.0)]);
    writer.add_way(Way::Road, &[at(5_000.0, 2_000.0), at(5_000.0, 4_000.0)]);
    writer
        .add_water(square(5_000.0, 3_000.0, 1_000.0))
        .await
        .expect("water");
    writer
        .add_coverage(Rect::new(at(-10_000.0, -10_000.0), at(10_000.0, 10_000.0)))
        .await
        .expect("coverage");
    writer.finish().await.expect("finished");
    path
}

#[tokio::test]
async fn us76_a_point_is_on_the_way_it_is_on_or_next_to() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let db = GroundDb::open(&a_world(dir.path()).await)
        .await
        .expect("opens")
        .expect("exists");
    let points = [
        at(2_000.0, 0.0),
        at(2_000.0, 15.0),
        at(2_000.0, 200.0),
        at(2_000.0, 1_000.0),
        at(4_800.0, 3_000.0),
        at(5_000.0, 3_000.0),
        at(20_000.0, 0.0),
    ];

    let data = db.around(&points).await.expect("read");
    let surfaces: Vec<Surface> = points.iter().map(|p| data.surface_at(*p)).collect();

    assert_eq!(
        surfaces,
        [
            Surface::Road,
            // A GPS a few metres off the road.
            Surface::Road,
            // Nowhere near a way.
            Surface::OffRoad,
            // On the path.
            Surface::OffRoad,
            Surface::Water,
            // The bridge.
            Surface::Road,
            // Beyond what the data covers.
            Surface::Unknown,
        ]
    );
}

#[tokio::test]
async fn us76_a_track_s_ground_is_the_share_of_its_length_on_each() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let db = GroundDb::open(&a_world(dir.path()).await)
        .await
        .expect("opens")
        .expect("exists");
    // 3 km along the road, about 3 km across country to the lake, then 1 km
    // across the lake beside the bridge.
    let across = |i: i32| {
        let t = f64::from(i) / 60.0;
        at(3_000.0 + 1_700.0 * t, 2_500.0 * t)
    };
    let track: Vec<Coord> = (0..60)
        .map(|i| at(f64::from(i) * 50.0, 0.0))
        .chain((0..60).map(across))
        .chain((0..=20).map(|i| at(4_700.0, 2_500.0 + f64::from(i) * 50.0)))
        .collect();
    let total = 3_000.0 + 1_700f64.hypot(2_500.0) + 1_000.0;

    let ground = db.around(&track).await.expect("read").ground_of(&track);

    assert!((ground.road - 3_000.0 / total).abs() < 0.02, "{ground:?}");
    assert!((ground.water - 1_000.0 / total).abs() < 0.02, "{ground:?}");
    assert!(
        (ground.off_road - 3_023.0 / total).abs() < 0.02,
        "{ground:?}"
    );
    assert!(ground.unknown == 0.0, "{ground:?}");
}

#[tokio::test]
async fn us76_a_fixture_keeps_what_is_around_its_tracks() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let full = a_world(dir.path()).await;
    let cut_path = dir.path().join("cut.sqlite");
    let mut cut = GroundWriter::create(&cut_path)
        .await
        .expect("a new database");
    cut.copy_within(
        &full,
        &[Rect::new(at(4_000.0, 2_000.0), at(6_000.0, 4_000.0))],
    )
    .await
    .expect("copied");
    cut.finish().await.expect("finished");

    let db = GroundDb::open(&cut_path)
        .await
        .expect("opens")
        .expect("exists");
    let points = [at(4_800.0, 3_000.0), at(5_000.0, 2_500.0), at(2_000.0, 0.0)];
    let data = db.around(&points).await.expect("read");

    assert_eq!(data.surface_at(points[0]), Surface::Water);
    assert_eq!(data.surface_at(points[1]), Surface::Road);
    // Cut away, so nothing there, though still covered.
    assert_eq!(data.surface_at(points[2]), Surface::OffRoad);
}

#[tokio::test]
async fn us76_a_place_database_is_not_a_ground_database() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let places = dir.path().join("places.sqlite");
    crate::server::places::PlaceWriter::create(&places)
        .await
        .expect("a new database")
        .finish()
        .await
        .expect("finished");

    assert!(GroundDb::open(&places).await.is_err());
}

#[test]
fn us76_a_road_beside_a_point_wins_over_a_path_beside_it_but_not_over_one_under_it() {
    use super::{Cell, GroundData, Tile};
    use std::collections::HashMap;

    let mut tiles: HashMap<i64, Tile> = HashMap::new();
    let mut mark = |cell: Cell, way: Way| tiles.entry(cell.tile()).or_default().set(cell, way);
    // Between a path and a road, on neither.
    let between = at(1_000.0, 1_000.0);
    let c = Cell::at(between);
    mark(Cell { x: c.x - 1, ..c }, Way::Small);
    mark(Cell { x: c.x + 1, ..c }, Way::Road);
    // On a path, a road beside it.
    let on_path = at(5_000.0, 5_000.0);
    let p = Cell::at(on_path);
    mark(p, Way::Small);
    mark(Cell { y: p.y + 1, ..p }, Way::Road);
    let covered = vec![Rect::new(at(-10_000.0, -10_000.0), at(10_000.0, 10_000.0))];

    let data = GroundData::new(tiles, Vec::new(), covered);

    assert_eq!(data.surface_at(between), Surface::Road);
    assert_eq!(data.surface_at(on_path), Surface::OffRoad);
}
