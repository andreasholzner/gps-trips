//! Test-only decoder for QMapShack waypoint blobs (US-89), as strict as the
//! track decoder: every byte of the `QMWpt` chunk must be accounted for.

use anyhow::{ensure, Context, Result};
use time::OffsetDateTime;

use super::{decode_history, lowercase_md5, quncompress, QtReader};
use crate::server::qmapshack::blob::waypoint::MAGIC_WPT;
use crate::server::qmapshack::blob::MAGIC_SIZE;
use crate::server::qmapshack::VER_WPT;

/// A decoded waypoint item: the fields of its blob the tests assert.
#[derive(Debug)]
pub struct DecodedWaypoint {
    pub keyqms: String,
    pub name: Option<String>,
    pub desc: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<i32>,
    pub time: Option<OffsetDateTime>,
    pub sym: Option<String>,
    pub keywords: Vec<String>,
    /// Embedded in the current history event; must equal `items.hash`.
    pub hash: String,
    pub who: Option<String>,
    /// MD5 recomputed over the current event's chunk bytes; must equal `hash`.
    pub chunk_md5: String,
}

/// Decode a full waypoint `items.data` blob (history_t → current event →
/// `QMWpt` chunk). Fails on any structural surprise, including leftover
/// bytes, and on a waypoint carrying geocache data or images, which the
/// exporter never writes.
pub fn decode_waypoint_item(data: &[u8]) -> Result<DecodedWaypoint> {
    let current = decode_history(data)?;
    let mut r = QtReader::new(&current.chunk);
    let magic = r.raw(MAGIC_SIZE).context("chunk magic")?;
    ensure!(
        magic == MAGIC_WPT,
        "not a QMWpt chunk (magic {:?})",
        String::from_utf8_lossy(magic)
    );
    let ver = r.u8().context("VER_WPT")?;
    ensure!(ver == VER_WPT, "unsupported VER_WPT {ver}");
    let compressed = r.qbytearray().context("compressed payload")?;
    ensure!(
        r.remaining() == 0,
        "{} unaccounted bytes after the compressed payload",
        r.remaining()
    );
    let inner = quncompress(&compressed)?;

    let mut r = QtReader::new(&inner);
    let keyqms = r.qstring().context("key.item")?.context("null key.item")?;
    let _flags = r.u32()?;
    let _proximity = r.f64()?;
    let wpt = r.wpt_base().context("wpt")?;
    let ver_gc = r.u8().context("geocache version")?;
    ensure!(ver_gc == 3, "unsupported VER_GC_T {ver_gc}");
    ensure!(r.u8()? == 0, "geocache data is not decoded");
    ensure!(r.u32()? == 0, "images are not decoded");
    let _offset_bubble = (r.i32()?, r.i32()?);
    let _width_bubble = r.u32()?;
    let _rating = r.f64()?;
    let n_keywords = r.u32().context("keywords count")?;
    let mut keywords = Vec::with_capacity(n_keywords as usize);
    for _ in 0..n_keywords {
        keywords.push(r.qstring()?.context("null keyword")?);
    }
    ensure!(
        r.remaining() == 0,
        "{} unaccounted bytes after the keywords",
        r.remaining()
    );

    Ok(DecodedWaypoint {
        keyqms,
        name: wpt.name,
        desc: wpt.desc,
        lat: wpt.point.lat,
        lon: wpt.point.lon,
        ele: wpt.point.ele,
        time: wpt.point.time,
        sym: wpt.sym,
        keywords,
        chunk_md5: lowercase_md5(&current.chunk),
        hash: current.hash,
        who: current.who,
    })
}

// ── Golden test against the owner's own database (ADR-0012) ─────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    /// The owner's real QMapShack database is not checked in (it is their
    /// own data, git-ignored), so this runs on demand where it is present:
    /// `cargo test --lib -- --ignored owner_waypoint`.
    #[tokio::test]
    #[ignore = "needs the git-ignored docs/qmapshack_analysis/Touren.db"]
    async fn us89_every_owner_waypoint_without_geocache_or_images_decodes_to_exhaustion() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/docs/qmapshack_analysis/Touren.db"
        );
        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(path)
                .read_only(true),
        )
        .await
        .expect("open Touren.db");
        let rows = sqlx::query("SELECT keyqms, name, data, hash FROM items WHERE type = 1")
            .fetch_all(&pool)
            .await
            .expect("read waypoints");
        assert!(!rows.is_empty(), "the database holds waypoints");

        for row in rows {
            let name: String = row.get("name");
            let decoded = decode_waypoint_item(&row.get::<Vec<u8>, _>("data"))
                .unwrap_or_else(|e| panic!("waypoint {name:?} failed to decode: {e:#}"));
            assert_eq!(decoded.keyqms, row.get::<String, _>("keyqms"), "{name:?}");
            assert_eq!(decoded.hash, row.get::<String, _>("hash"), "{name:?}");
            assert_eq!(decoded.chunk_md5, decoded.hash, "{name:?}");
        }
    }
}
