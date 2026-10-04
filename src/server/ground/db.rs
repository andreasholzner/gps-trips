//! The ground database: tiles of ways by id, water pieces under an R*Tree,
//! and the boxes the extracts it was built from cover — in a SQLite file of
//! its own (ADR-0027), read-only for the server, written by
//! `places_build ground` and nothing else.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use geo::{BoundingRect, Coord, MultiPolygon, Polygon, Rect};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::{tile::cells_along, water, Cell, GroundData, Tile, Way};
use crate::server::places::{decode_area, encode_area};

/// At most this many points in a stored water piece.
const WATER_PIECE_POINTS: usize = 1_000;

/// Tile ids per query, under SQLite's limit on bound values.
const TILES_PER_QUERY: usize = 500;

const SCHEMA: &str = "
CREATE TABLE tile (
    id   INTEGER PRIMARY KEY,
    ways BLOB    NOT NULL
);
CREATE TABLE water (
    id    INTEGER PRIMARY KEY,
    piece BLOB    NOT NULL
);
CREATE VIRTUAL TABLE water_bbox USING rtree(id, min_lon, max_lon, min_lat, max_lat);
CREATE TABLE coverage (
    min_lon REAL NOT NULL,
    min_lat REAL NOT NULL,
    max_lon REAL NOT NULL,
    max_lat REAL NOT NULL
);
";

/// The ground database the server reads.
#[derive(Clone)]
pub struct GroundDb {
    pool: SqlitePool,
    coverage: Vec<Rect>,
}

impl GroundDb {
    /// Opens the database at `path` read-only; `Ok(None)` when there is no
    /// file there, which is a supported way to run (ADR-0027).
    pub async fn open(path: &Path) -> Result<Option<Self>, sqlx::Error> {
        if !path.is_file() {
            return Ok(None);
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .read_only(true)
            .immutable(true);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
        let coverage = sqlx::query("SELECT min_lon, min_lat, max_lon, max_lat FROM coverage")
            .fetch_all(&pool)
            .await?
            .iter()
            .map(|row| {
                Rect::new(
                    Coord {
                        x: row.get("min_lon"),
                        y: row.get("min_lat"),
                    },
                    Coord {
                        x: row.get("max_lon"),
                        y: row.get("max_lat"),
                    },
                )
            })
            .collect();
        // Fails here, at boot, for a file that is not a ground database.
        sqlx::query("SELECT 1 FROM tile, water, water_bbox LIMIT 1")
            .fetch_optional(&pool)
            .await?;
        Ok(Some(Self { pool, coverage }))
    }

    /// What the ground data knows around the track through `coords`.
    pub async fn around(&self, coords: &[Coord]) -> Result<GroundData, sqlx::Error> {
        let ids: Vec<i64> = coords
            .iter()
            .flat_map(|c| Cell::at(*c).with_neighbours())
            .map(Cell::tile)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let mut tiles = HashMap::new();
        for chunk in ids.chunks(TILES_PER_QUERY) {
            let marks = vec!["?"; chunk.len()].join(",");
            let sql = format!("SELECT id, ways FROM tile WHERE id IN ({marks})");
            let mut query = sqlx::query(&sql);
            for id in chunk {
                query = query.bind(id);
            }
            for row in query.fetch_all(&self.pool).await? {
                if let Some(tile) = Tile::decode(row.get("ways")) {
                    tiles.insert(row.get::<i64, _>("id"), tile);
                }
            }
        }

        let mut seen = HashSet::new();
        let mut pieces = Vec::new();
        for rect in crate::server::track_boxes::around(coords, 0.0) {
            let rows = sqlx::query(
                "SELECT w.id, w.piece FROM water_bbox b JOIN water w ON w.id = b.id \
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
                    pieces.extend(decode_area(row.get("piece")).into_iter().flat_map(|a| a.0));
                }
            }
        }
        Ok(GroundData::new(tiles, pieces, self.coverage.clone()))
    }
}

/// Builds a ground database: `places_build ground`, and the tests'
/// fixtures. The ways are gathered in memory — two regions' extracts meet
/// in the same tiles — and written by [`GroundWriter::finish`].
pub struct GroundWriter {
    conn: SqliteConnection,
    tiles: HashMap<i64, Tile>,
    coverage: Vec<Rect>,
}

impl GroundWriter {
    /// Creates an empty ground database at `path`, which must not exist yet.
    pub async fn create(path: &Path) -> Result<Self, sqlx::Error> {
        use sqlx::ConnectOptions;
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Off)
            .synchronous(SqliteSynchronous::Off);
        let mut conn = options.connect().await?;
        if sqlx::query("SELECT 1 FROM sqlite_master LIMIT 1")
            .fetch_optional(&mut conn)
            .await?
            .is_some()
        {
            return Err(sqlx::Error::Protocol(format!(
                "{} already holds a database",
                path.display()
            )));
        }
        sqlx::raw_sql(SCHEMA).execute(&mut conn).await?;
        sqlx::query("BEGIN").execute(&mut conn).await?;
        Ok(Self {
            conn,
            tiles: HashMap::new(),
            coverage: Vec::new(),
        })
    }

    /// Marks every cell the way through `coords` crosses.
    pub fn add_way(&mut self, way: Way, coords: &[Coord]) {
        for pair in coords.windows(2) {
            for cell in cells_along(pair[0], pair[1]) {
                self.tiles.entry(cell.tile()).or_default().set(cell, way);
            }
        }
    }

    /// Adds a body of water, in pieces.
    pub async fn add_water(&mut self, polygon: Polygon) -> Result<(), sqlx::Error> {
        for piece in water::split(polygon, WATER_PIECE_POINTS) {
            let Some(rect) = piece.bounding_rect() else {
                continue;
            };
            let id = sqlx::query("INSERT INTO water (piece) VALUES (?)")
                .bind(encode_area(&MultiPolygon(vec![piece])))
                .execute(&mut self.conn)
                .await?
                .last_insert_rowid();
            sqlx::query(
                "INSERT INTO water_bbox (id, min_lon, max_lon, min_lat, max_lat) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(rect.min().x)
            .bind(rect.max().x)
            .bind(rect.min().y)
            .bind(rect.max().y)
            .execute(&mut self.conn)
            .await?;
        }
        Ok(())
    }

    /// What the extracts added so far cover.
    pub fn coverage(&self) -> &[Rect] {
        &self.coverage
    }

    /// Records that the data covers `rect`: an extract's bounding box.
    pub async fn add_coverage(&mut self, rect: Rect) -> Result<(), sqlx::Error> {
        self.coverage.push(rect);
        sqlx::query(
            "INSERT INTO coverage (min_lon, min_lat, max_lon, max_lat) VALUES (?, ?, ?, ?)",
        )
        .bind(rect.min().x)
        .bind(rect.min().y)
        .bind(rect.max().x)
        .bind(rect.max().y)
        .execute(&mut self.conn)
        .await?;
        Ok(())
    }

    /// Copies what the database at `source` knows within `boxes`: a test
    /// fixture cut from the real database. Its coverage is copied whole.
    pub async fn copy_within(&mut self, source: &Path, boxes: &[Rect]) -> Result<(), sqlx::Error> {
        sqlx::query("COMMIT").execute(&mut self.conn).await?;
        sqlx::query("ATTACH DATABASE ? AS src")
            .bind(source.display().to_string())
            .execute(&mut self.conn)
            .await?;
        sqlx::query("BEGIN").execute(&mut self.conn).await?;
        for rect in boxes {
            // Rows count southwards: the box's north-east corner has the
            // lower one.
            let (x0, y1) = Cell::at(rect.min()).tile_xy();
            let (x1, y0) = Cell::at(rect.max()).tile_xy();
            for row in sqlx::query(
                "SELECT id, ways FROM src.tile \
                 WHERE (id >> 16) BETWEEN ? AND ? AND (id & 65535) BETWEEN ? AND ?",
            )
            .bind(x0)
            .bind(x1)
            .bind(y0)
            .bind(y1)
            .fetch_all(&mut self.conn)
            .await?
            {
                if let Some(tile) = Tile::decode(row.get("ways")) {
                    self.tiles.insert(row.get("id"), tile);
                }
            }
            sqlx::query(
                "INSERT OR IGNORE INTO water (id, piece) \
                 SELECT w.id, w.piece FROM src.water_bbox b JOIN src.water w ON w.id = b.id \
                 WHERE b.max_lon >= ? AND b.min_lon <= ? AND b.max_lat >= ? AND b.min_lat <= ?",
            )
            .bind(rect.min().x)
            .bind(rect.max().x)
            .bind(rect.min().y)
            .bind(rect.max().y)
            .execute(&mut self.conn)
            .await?;
        }
        sqlx::query(
            "INSERT INTO water_bbox (id, min_lon, max_lon, min_lat, max_lat) \
             SELECT b.id, b.min_lon, b.max_lon, b.min_lat, b.max_lat FROM src.water_bbox b \
             WHERE b.id IN (SELECT id FROM water) AND b.id NOT IN (SELECT id FROM water_bbox)",
        )
        .execute(&mut self.conn)
        .await?;
        sqlx::query(
            "INSERT INTO coverage SELECT * FROM src.coverage \
             WHERE NOT EXISTS (SELECT 1 FROM coverage)",
        )
        .execute(&mut self.conn)
        .await?;
        sqlx::query("COMMIT").execute(&mut self.conn).await?;
        sqlx::query("DETACH DATABASE src")
            .execute(&mut self.conn)
            .await?;
        sqlx::query("BEGIN").execute(&mut self.conn).await?;
        Ok(())
    }

    /// Writes the ways, commits and compacts the file.
    pub async fn finish(mut self) -> Result<(), sqlx::Error> {
        for (id, tile) in &self.tiles {
            sqlx::query("INSERT INTO tile (id, ways) VALUES (?, ?)")
                .bind(id)
                .bind(tile.encode())
                .execute(&mut self.conn)
                .await?;
        }
        sqlx::query("COMMIT").execute(&mut self.conn).await?;
        sqlx::query("VACUUM").execute(&mut self.conn).await?;
        Ok(())
    }
}
