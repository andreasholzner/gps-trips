//! The place database itself: one table of places and an R*Tree over their
//! bounding boxes, in a SQLite file of its own (ADR-0027) — read-only for
//! the server, written by `places_build` and nothing else.

use std::collections::HashSet;
use std::path::Path;

use geo::{BoundingRect, Coord, Rect};
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::shape::{decode_area, encode_area};
use super::{Place, PlaceKind, Shape, Source};
use crate::server::geodata;

const SCHEMA: &str = "
CREATE TABLE place (
    id           INTEGER PRIMARY KEY,
    source       TEXT    NOT NULL,
    source_id    TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    lon          REAL,
    lat          REAL,
    area         BLOB,
    ele_m        REAL,
    prominence_m REAL,
    area_m2      REAL,
    population   INTEGER,
    UNIQUE (source, source_id)
);
CREATE VIRTUAL TABLE place_bbox USING rtree(id, min_lon, max_lon, min_lat, max_lat);
";

/// The place database the server reads.
#[derive(Clone)]
pub struct PlaceDb {
    pool: SqlitePool,
}

impl PlaceDb {
    /// Opens the database at `path` read-only; `Ok(None)` when there is no
    /// file there, which is a supported way to run (ADR-0027).
    pub async fn open(path: &Path) -> Result<Option<Self>, sqlx::Error> {
        let pool = geodata::open_read_only(path, "SELECT 1 FROM place, place_bbox LIMIT 1").await?;
        Ok(pool.map(|pool| Self { pool }))
    }

    /// Every place whose bounding box meets one of `boxes` (longitude/
    /// latitude), each once.
    pub async fn within(&self, boxes: &[Rect]) -> Result<Vec<Place>, sqlx::Error> {
        let mut seen = HashSet::new();
        let mut places = Vec::new();
        for rect in boxes {
            let rows = sqlx::query(
                "SELECT p.id, p.source, p.name, p.kind, p.lon, p.lat, p.area, \
                        p.ele_m, p.prominence_m, p.area_m2, p.population \
                 FROM place_bbox b JOIN place p ON p.id = b.id \
                 WHERE b.max_lon >= ? AND b.min_lon <= ? AND b.max_lat >= ? AND b.min_lat <= ?",
            )
            .bind(rect.min().x)
            .bind(rect.max().x)
            .bind(rect.min().y)
            .bind(rect.max().y)
            .fetch_all(&self.pool)
            .await?;
            for row in rows {
                if seen.insert(row.get::<i64, _>("id")) {
                    places.extend(place_from_row(&row));
                }
            }
        }
        Ok(places)
    }
}

/// A stored place; `None` for a row this server does not understand, which
/// is skipped rather than failing the suggestion.
fn place_from_row(row: &sqlx::sqlite::SqliteRow) -> Option<Place> {
    let shape = match row.get::<Option<Vec<u8>>, _>("area") {
        Some(bytes) => Shape::Area(decode_area(&bytes)?),
        None => Shape::Point(Coord {
            x: row.get::<Option<f64>, _>("lon")?,
            y: row.get::<Option<f64>, _>("lat")?,
        }),
    };
    Some(Place {
        name: row.get("name"),
        kind: PlaceKind::parse(row.get("kind"))?,
        source: Source::parse(row.get("source"))?,
        shape,
        ele_m: row.get("ele_m"),
        prominence_m: row.get("prominence_m"),
        area_m2: row.get("area_m2"),
        population: row
            .get::<Option<i64>, _>("population")
            .and_then(|n| u64::try_from(n).ok()),
    })
}

/// Builds a place database: `places_build`, and the tests' fixtures.
pub struct PlaceWriter {
    conn: SqliteConnection,
}

impl PlaceWriter {
    /// Creates an empty place database at `path`, which must not exist yet.
    pub async fn create(path: &Path) -> Result<Self, sqlx::Error> {
        Ok(Self {
            conn: geodata::create(path, SCHEMA).await?,
        })
    }

    /// Adds `place`, known to its source as `source_id`; a place already
    /// added under that id — two regions' extracts overlap at their
    /// borders — is skipped.
    pub async fn insert(&mut self, place: &Place, source_id: &str) -> Result<(), sqlx::Error> {
        let (lon, lat, area, rect) = match &place.shape {
            Shape::Point(coord) => (
                Some(coord.x),
                Some(coord.y),
                None,
                Rect::new(*coord, *coord),
            ),
            Shape::Area(area) => {
                let Some(rect) = area.bounding_rect() else {
                    return Ok(());
                };
                (None, None, Some(encode_area(area)), rect)
            }
        };
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO place \
             (source, source_id, name, kind, lon, lat, area, ele_m, prominence_m, area_m2, population) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(place.source.as_str())
        .bind(source_id)
        .bind(&place.name)
        .bind(place.kind.as_str())
        .bind(lon)
        .bind(lat)
        .bind(area)
        .bind(place.ele_m)
        .bind(place.prominence_m)
        .bind(place.area_m2)
        .bind(place.population.and_then(|n| i64::try_from(n).ok()))
        .execute(&mut self.conn)
        .await?;
        if inserted.rows_affected() == 1 {
            sqlx::query(
                "INSERT INTO place_bbox (id, min_lon, max_lon, min_lat, max_lat) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(inserted.last_insert_rowid())
            .bind(rect.min().x)
            .bind(rect.max().x)
            .bind(rect.min().y)
            .bind(rect.max().y)
            .execute(&mut self.conn)
            .await?;
        }
        Ok(())
    }

    /// Copies every place of the database at `source` whose bounding box
    /// meets one of `boxes`: a test fixture cut from the real database.
    pub async fn copy_within(&mut self, source: &Path, boxes: &[Rect]) -> Result<(), sqlx::Error> {
        geodata::attach_source(&mut self.conn, source).await?;
        for rect in boxes {
            sqlx::query(
                "INSERT OR IGNORE INTO place \
                 (source, source_id, name, kind, lon, lat, area, ele_m, prominence_m, area_m2, population) \
                 SELECT p.source, p.source_id, p.name, p.kind, p.lon, p.lat, p.area, \
                        p.ele_m, p.prominence_m, p.area_m2, p.population \
                 FROM src.place_bbox b JOIN src.place p ON p.id = b.id \
                 WHERE b.max_lon >= ?1 AND b.min_lon <= ?2 AND b.max_lat >= ?3 AND b.min_lat <= ?4",
            )
            .bind(rect.min().x)
            .bind(rect.max().x)
            .bind(rect.min().y)
            .bind(rect.max().y)
            .execute(&mut self.conn)
            .await?;
        }
        sqlx::query(
            "INSERT INTO place_bbox (id, min_lon, max_lon, min_lat, max_lat) \
             SELECT p.id, b.min_lon, b.max_lon, b.min_lat, b.max_lat \
             FROM place p \
             JOIN src.place s ON s.source = p.source AND s.source_id = p.source_id \
             JOIN src.place_bbox b ON b.id = s.id \
             WHERE p.id NOT IN (SELECT id FROM place_bbox)",
        )
        .execute(&mut self.conn)
        .await?;
        geodata::detach_source(&mut self.conn).await
    }

    /// Commits what was added and compacts the file.
    pub async fn finish(self) -> Result<(), sqlx::Error> {
        geodata::finish(self.conn).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::polygon;

    fn summit(name: &str, x: f64, y: f64) -> Place {
        Place {
            name: name.to_string(),
            kind: PlaceKind::Summit,
            source: Source::Osm,
            shape: Shape::Point(Coord { x, y }),
            ele_m: Some(884.0),
            prominence_m: None,
            area_m2: None,
            population: None,
        }
    }

    fn lake(name: &str) -> Place {
        Place {
            name: name.to_string(),
            kind: PlaceKind::Lake,
            source: Source::Osm,
            shape: Shape::Area(geo::MultiPolygon(vec![polygon![
                (x: 10.0, y: 60.0),
                (x: 10.1, y: 60.0),
                (x: 10.1, y: 60.1),
                (x: 10.0, y: 60.0),
            ]])),
            ele_m: None,
            prominence_m: None,
            area_m2: Some(1_000_000.0),
            population: None,
        }
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        Rect::new(Coord { x: x0, y: y0 }, Coord { x: x1, y: y1 })
    }

    async fn written(dir: &Path, places: &[(Place, &str)]) -> std::path::PathBuf {
        let path = dir.join("places.sqlite");
        let mut writer = PlaceWriter::create(&path).await.expect("a new database");
        for (place, id) in places {
            writer.insert(place, id).await.expect("inserted");
        }
        writer.finish().await.expect("finished");
        path
    }

    #[tokio::test]
    async fn us74_places_are_found_by_the_boxes_they_meet() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = written(
            dir.path(),
            &[
                (summit("Near", 10.05, 60.05), "node/1"),
                (summit("Far", 12.0, 62.0), "node/2"),
                (lake("Vatnet"), "way/3"),
            ],
        )
        .await;

        let db = PlaceDb::open(&path).await.expect("opens").expect("exists");
        let mut names: Vec<String> = db
            .within(&[
                rect(10.04, 60.04, 10.06, 60.06),
                rect(10.05, 60.05, 10.07, 60.07),
            ])
            .await
            .expect("read")
            .into_iter()
            .map(|p| p.name)
            .collect();
        names.sort();

        assert_eq!(names, ["Near", "Vatnet"]);
    }

    #[tokio::test]
    async fn us74_a_place_two_extracts_hold_is_stored_once() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = written(
            dir.path(),
            &[
                (summit("Twice", 10.0, 60.0), "node/1"),
                (summit("Twice", 10.0, 60.0), "node/1"),
            ],
        )
        .await;

        let db = PlaceDb::open(&path).await.expect("opens").expect("exists");
        let found = db
            .within(&[rect(9.0, 59.0, 11.0, 61.0)])
            .await
            .expect("read");

        assert_eq!(found, [summit("Twice", 10.0, 60.0)]);
    }

    #[tokio::test]
    async fn us74_a_fixture_is_cut_from_the_places_around_the_tracks() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let full = written(
            dir.path(),
            &[
                (summit("Near", 10.05, 60.05), "node/1"),
                (summit("Far", 12.0, 62.0), "node/2"),
            ],
        )
        .await;
        let cut_path = dir.path().join("cut.sqlite");
        let mut cut = PlaceWriter::create(&cut_path)
            .await
            .expect("a new database");
        cut.copy_within(&full, &[rect(10.0, 60.0, 10.1, 60.1)])
            .await
            .expect("copied");
        cut.finish().await.expect("finished");

        let db = PlaceDb::open(&cut_path)
            .await
            .expect("opens")
            .expect("exists");
        let found = db
            .within(&[rect(-180.0, -90.0, 180.0, 90.0)])
            .await
            .expect("read");

        assert_eq!(found, [summit("Near", 10.05, 60.05)]);
    }

    #[tokio::test]
    async fn us74_a_ground_database_is_not_a_place_database() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let ground = dir.path().join("ground.sqlite");
        crate::server::ground::GroundWriter::create(&ground)
            .await
            .expect("a new database")
            .finish()
            .await
            .expect("finished");

        assert!(PlaceDb::open(&ground).await.is_err());
    }
}
