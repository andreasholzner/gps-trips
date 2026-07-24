//! Test-only decoder for QMapShack `items.data` track blobs: golden tests
//! decode the reference `format_test.db` items, and the US-36 acceptance
//! tests decode what the exporter wrote. Exposed to `tests/` via the
//! `test-support` feature (same pattern as `location::fixtures`). Never used
//! by the exporter itself — writing stays one-way.
//!
//! The decoder is deliberately strict: every byte of the outer history
//! stream and of the current event's track chunk must be accounted for, so
//! a golden decode doubles as proof that the writer's field list matches
//! QMapShack's (`serialization.cpp`).

use std::io::Read;

use anyhow::{bail, ensure, Context, Result};
use md5::{Digest, Md5};
use time::OffsetDateTime;

use super::blob::{MAGIC_SIZE, MAGIC_TRK, NOINT};
use super::VER_TRK;

/// One decoded track point (the subset of `trkpt_t` the archive round-trips).
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedPoint {
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<i32>,
    pub time: Option<OffsetDateTime>,
}

/// A decoded track item: the fields of the `history_t` blob the tests assert.
#[derive(Debug)]
pub struct DecodedTrack {
    pub keyqms: String,
    pub name: Option<String>,
    pub desc: Option<String>,
    pub trk_type: Option<String>,
    pub color: Option<String>,
    pub keywords: Vec<String>,
    /// Lowercase MD5 hex embedded in the current history event — QMapShack
    /// requires it to equal `items.hash`.
    pub hash: String,
    pub who: Option<String>,
    /// MD5 recomputed over the current event's chunk bytes; must equal `hash`.
    pub chunk_md5: String,
    /// All points across all segments, in stream order.
    pub points: Vec<DecodedPoint>,
}

/// Decode a full `items.data` blob (history_t → current event → `QMTrk`
/// chunk). Fails on any structural surprise, including leftover bytes.
pub fn decode_track_item(data: &[u8]) -> Result<DecodedTrack> {
    let mut r = QtReader::new(data);

    let ver_hist = r.u8().context("VER_HIST")?;
    ensure!(ver_hist == 1, "unexpected VER_HIST {ver_hist}");
    let _idx_initial = r.i32()?;
    let idx_current = r.i32().context("histIdxCurrent")?;
    let n_events = r.u32().context("event count")?;
    ensure!(
        idx_current >= 0 && (idx_current as u32) < n_events,
        "histIdxCurrent {idx_current} out of range for {n_events} events"
    );

    let mut current: Option<(Vec<u8>, String, Option<String>)> = None;
    for i in 0..n_events {
        let ver_evt = r.u8().with_context(|| format!("event {i} version"))?;
        ensure!(ver_evt == 3, "unsupported VER_HIST_EVT {ver_evt}");
        let _time = r.qdatetime().with_context(|| format!("event {i} time"))?;
        let _icon = r.qstring()?;
        let _comment = r.qstring()?;
        let chunk = r.qbytearray().with_context(|| format!("event {i} data"))?;
        let hash = r
            .qstring()?
            .with_context(|| format!("event {i} has a null hash"))?;
        let who = r.qstring()?;
        if i == idx_current as u32 {
            current = Some((chunk, hash, who));
        }
    }
    ensure!(
        r.remaining() == 0,
        "{} unaccounted bytes after the history events",
        r.remaining()
    );

    let (chunk, hash, who) = current.expect("current index validated against event count");
    let chunk_md5 = lowercase_md5(&chunk);
    let mut track = decode_track_chunk(&chunk)?;
    track.hash = hash;
    track.who = who;
    track.chunk_md5 = chunk_md5;
    Ok(track)
}

/// Decode the `QMTrk` chunk of one history event (`CGisItemTrk::operator>>`'s
/// write path, `serialization.cpp`).
fn decode_track_chunk(chunk: &[u8]) -> Result<DecodedTrack> {
    let mut r = QtReader::new(chunk);
    let magic = r.raw(MAGIC_SIZE).context("chunk magic")?;
    ensure!(
        magic == MAGIC_TRK,
        "not a QMTrk chunk (magic {:?})",
        String::from_utf8_lossy(magic)
    );
    let ver = r.u8().context("VER_TRK")?;
    ensure!(ver == VER_TRK, "unsupported VER_TRK {ver}");
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
    let name = r.qstring().context("trk.name")?;
    let _cmt = r.qstring()?;
    let desc = r.qstring().context("trk.desc")?;
    let _src = r.qstring()?;
    r.links().context("trk.links")?;
    let _number = r.u64()?;
    let trk_type = r.qstring().context("trk.type")?;
    let color = r.qstring().context("trk.color")?;
    let _rating = r.f64()?;
    let n_keywords = r.u32().context("keywords count")?;
    let mut keywords = Vec::with_capacity(n_keywords as usize);
    for _ in 0..n_keywords {
        keywords.push(r.qstring()?.context("null keyword")?);
    }

    r.climit().context("colorSourceLimit")?;
    r.cvalue().context("lineScale")?;
    r.cvalue().context("showArrows")?;
    r.climit().context("limitsGraph1")?;
    r.climit().context("limitsGraph2")?;
    r.climit().context("limitsGraph3")?;
    r.energy_set().context("energyCycling")?;

    let n_segs = r.u32().context("segment count")?;
    let mut points = Vec::new();
    for seg in 0..n_segs {
        let ver_seg = r.u8().with_context(|| format!("segment {seg} version"))?;
        ensure!(ver_seg == 1, "unsupported VER_TRKSEG {ver_seg}");
        let n_pts = r.u32()?;
        for pt in 0..n_pts {
            points.push(
                r.trkpt()
                    .with_context(|| format!("segment {seg} point {pt}"))?,
            );
        }
    }
    ensure!(
        r.remaining() == 0,
        "{} unaccounted bytes after trk.segs",
        r.remaining()
    );

    Ok(DecodedTrack {
        keyqms,
        name,
        desc,
        trk_type,
        color,
        keywords,
        hash: String::new(),
        who: None,
        chunk_md5: String::new(),
        points,
    })
}

pub fn lowercase_md5(bytes: &[u8]) -> String {
    let digest = Md5::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Qt's `qUncompress`: 4-byte big-endian uncompressed length + zlib stream.
fn quncompress(framed: &[u8]) -> Result<Vec<u8>> {
    ensure!(framed.len() > 4, "qCompress data too short");
    let declared = u32::from_be_bytes(framed[..4].try_into().unwrap()) as usize;
    let mut out = Vec::with_capacity(declared);
    flate2::read::ZlibDecoder::new(&framed[4..])
        .read_to_end(&mut out)
        .context("zlib stream inside qCompress framing")?;
    ensure!(
        out.len() == declared,
        "qCompress declared {declared} bytes but inflated {}",
        out.len()
    );
    Ok(out)
}

/// Cursor over a Qt_5_2 little-endian `QDataStream`.
struct QtReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> QtReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn raw(&mut self, n: usize) -> Result<&'a [u8]> {
        ensure!(
            self.remaining() >= n,
            "stream truncated at offset {} (wanted {n} bytes, {} left)",
            self.pos,
            self.remaining()
        );
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.raw(1)?[0])
    }

    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.raw(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.raw(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.raw(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.raw(8)?.try_into().unwrap()))
    }

    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.raw(8)?.try_into().unwrap()))
    }

    fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.raw(8)?.try_into().unwrap()))
    }

    fn qstring(&mut self) -> Result<Option<String>> {
        let len = self.u32()?;
        if len == 0xFFFF_FFFF {
            return Ok(None);
        }
        ensure!(len % 2 == 0, "odd QString byte length {len}");
        let bytes = self.raw(len as usize)?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Ok(Some(String::from_utf16(&units).context("invalid UTF-16")?))
    }

    fn qbytearray(&mut self) -> Result<Vec<u8>> {
        let len = self.u32()?;
        if len == 0xFFFF_FFFF {
            return Ok(Vec::new());
        }
        Ok(self.raw(len as usize)?.to_vec())
    }

    fn qdatetime(&mut self) -> Result<Option<OffsetDateTime>> {
        let jd = self.i64()?;
        let ms = self.u32()?;
        let spec = self.u8()?;
        // Extra payload per spec, present even alongside a null date/time.
        let offset_secs = match spec {
            0 | 1 => 0,
            2 => self.i32()?,
            other => bail!("unsupported QDateTime spec {other}"),
        };
        if jd == i64::MIN || ms == 0xFFFF_FFFF {
            return Ok(None);
        }
        let jd = i32::try_from(jd).context("julian day out of range")?;
        let date = time::Date::from_julian_day(jd).context("invalid julian day")?;
        let time = time::Time::MIDNIGHT + time::Duration::milliseconds(i64::from(ms));
        let utc_offset =
            time::UtcOffset::from_whole_seconds(offset_secs).context("invalid UTC offset")?;
        Ok(Some(
            date.with_time(time)
                .assume_offset(utc_offset)
                .to_offset(time::UtcOffset::UTC),
        ))
    }

    /// `QList<link_t>` — decoded and discarded.
    fn links(&mut self) -> Result<()> {
        let n = self.u32()?;
        for i in 0..n {
            let ver = self.u8().with_context(|| format!("link {i} version"))?;
            ensure!(ver == 1, "unsupported VER_LINK {ver}");
            self.qstring()?; // uri
            self.qstring()?; // text
            self.qstring()?; // type
        }
        Ok(())
    }

    /// `QVariant`: type id + isNull flag + payload — decoded and discarded.
    fn qvariant(&mut self) -> Result<()> {
        let type_id = self.u32()?;
        let _is_null = self.u8()?;
        match type_id {
            0 => {}                         // Invalid: no payload
            1 => drop(self.u8()?),          // Bool
            2 => drop(self.i32()?),         // Int
            3 => drop(self.u32()?),         // UInt
            4 => drop(self.i64()?),         // LongLong
            5 => drop(self.u64()?),         // ULongLong
            6 => drop(self.f64()?),         // Double
            10 => drop(self.qstring()?),    // QString
            12 => drop(self.qbytearray()?), // QByteArray
            other => bail!("unsupported QVariant type id {other}"),
        }
        Ok(())
    }

    /// `CLimit` — decoded and discarded.
    fn climit(&mut self) -> Result<()> {
        let ver = self.u8()?;
        ensure!(ver == 1, "unsupported VER_CLIMIT {ver}");
        self.u8()?; // mode
        self.qstring()?; // source
        self.f64()?; // minUser
        self.f64()?; // maxUser
        Ok(())
    }

    /// `CValue` — decoded and discarded.
    fn cvalue(&mut self) -> Result<()> {
        let ver = self.u8()?;
        ensure!(ver == 1, "unsupported VER_CVALUE {ver}");
        self.u8()?; // mode
        self.qvariant() // valUser
    }

    /// `energy_set_t` — decoded and discarded.
    fn energy_set(&mut self) -> Result<()> {
        let ver = self.u8()?;
        ensure!(ver == 1, "unsupported VER_ENERGYCYCLE {ver}");
        for _ in 0..3 {
            self.f64()?; // driverWeight, bikeWeight, airDensity
        }
        self.i32()?; // windSpeedIndex
        self.f64()?; // windSpeed
        self.i32()?; // windPositionIndex
        self.f64()?; // frontalArea
        self.f64()?; // windDragCoeff
        self.i32()?; // groundIndex
        self.f64()?; // rollingCoeff
        self.f64()?; // pedalCadence
        self.f64()?; // energyKcal
        Ok(())
    }

    /// One `trkpt_t` (version 3): flags + `wpt_t` base + extensions + activity.
    fn trkpt(&mut self) -> Result<DecodedPoint> {
        let ver = self.u8()?;
        ensure!(ver == 3, "unsupported VER_TRKPT {ver}");
        self.u32()?; // flags

        let ver_wpt = self.u8()?;
        ensure!(ver_wpt == 1, "unsupported VER_WPT_T {ver_wpt}");
        let lat = self.f64()?;
        let lon = self.f64()?;
        let ele = self.i32()?;
        let time = self.qdatetime()?;
        self.i32()?; // magvar
        self.i32()?; // geoidheight
        for _ in 0..4 {
            self.qstring()?; // name, cmt, desc, src
        }
        self.links()?;
        for _ in 0..3 {
            self.qstring()?; // sym, type, fix
        }
        for _ in 0..6 {
            self.i32()?; // sat, hdop, vdop, pdop, ageofdgpsdata, dgpsid
        }

        let n_ext = self.u32()?; // QHash<QString, QVariant> extensions
        for _ in 0..n_ext {
            self.qstring()?;
            self.qvariant()?;
        }
        self.i16()?; // activity

        Ok(DecodedPoint {
            lat,
            lon,
            ele: (ele != NOINT).then_some(ele),
            time,
        })
    }
}

// ── Golden tests against the reference database (ADR-0012) ──────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    /// The reference QMapShack database checked into the repo (3 tracks,
    /// written by real QMapShack — `docs/qmapshack-format.md`).
    async fn format_test_db() -> sqlx::SqlitePool {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/docs/qmapshack_analysis/format_test.db"
        );
        sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(path)
                .read_only(true),
        )
        .await
        .expect("open format_test.db")
    }

    #[tokio::test]
    async fn every_reference_track_blob_decodes_to_exhaustion() {
        let pool = format_test_db().await;
        let rows = sqlx::query("SELECT keyqms, name, data, hash FROM items WHERE type = 2")
            .fetch_all(&pool)
            .await
            .expect("read reference items");
        assert_eq!(rows.len(), 3, "reference DB holds 3 tracks");

        for row in rows {
            let name: String = row.get("name");
            let decoded = decode_track_item(&row.get::<Vec<u8>, _>("data"))
                .unwrap_or_else(|e| panic!("track {name:?} failed to decode: {e:#}"));
            assert!(!decoded.points.is_empty(), "track {name:?} has points");
            assert_eq!(
                decoded.keyqms,
                row.get::<String, _>("keyqms"),
                "embedded key matches items.keyqms for {name:?}"
            );
        }
    }

    #[tokio::test]
    async fn embedded_hash_is_md5_of_the_chunk_and_matches_items_hash() {
        let pool = format_test_db().await;
        let rows = sqlx::query("SELECT name, data, hash FROM items WHERE type = 2")
            .fetch_all(&pool)
            .await
            .expect("read reference items");

        for row in rows {
            let name: String = row.get("name");
            let decoded = decode_track_item(&row.get::<Vec<u8>, _>("data"))
                .unwrap_or_else(|e| panic!("track {name:?} failed to decode: {e:#}"));
            let sql_hash: String = row.get("hash");
            assert_eq!(
                decoded.hash, sql_hash,
                "embedded vs items.hash for {name:?}"
            );
            assert_eq!(
                decoded.chunk_md5, sql_hash,
                "md5(chunk) vs items.hash for {name:?}"
            );
        }
    }
}
