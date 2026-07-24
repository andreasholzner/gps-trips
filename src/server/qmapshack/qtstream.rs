//! Qt `QDataStream` primitive encoding, pinned to what QMapShack pins for
//! its database blobs: byte order `LittleEndian`, stream version `Qt_5_2`
//! (`docs/qmapshack-format.md`). Only the writing side the exporter needs —
//! plus `qcompress`, Qt's compression framing used inside track blobs.
//!
//! Golden byte values in the tests below were taken from decoding real
//! QMapShack databases (`docs/qmapshack_analysis/format_test.db`,
//! `Touren.db`) during US-36 planning, not derived from this code.

use std::io::Write;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use time::OffsetDateTime;

/// Byte length prefix Qt uses for a null `QString`/`QByteArray`.
const NULL_LENGTH: u32 = 0xFFFF_FFFF;

/// Accumulates a Qt_5_2 little-endian `QDataStream`.
#[derive(Default)]
pub struct QtWriter {
    buf: Vec<u8>,
}

impl QtWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn raw(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn i16(&mut self, v: i16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// `QString`: `u32` byte length + UTF-16 code units in stream byte order
    /// (little-endian here). `None` encodes Qt's null string.
    pub fn qstring(&mut self, s: Option<&str>) {
        match s {
            None => self.u32(NULL_LENGTH),
            Some(s) => {
                let units: Vec<u16> = s.encode_utf16().collect();
                self.u32((units.len() * 2) as u32);
                for unit in units {
                    self.buf.extend_from_slice(&unit.to_le_bytes());
                }
            }
        }
    }

    /// `QByteArray`: `u32` byte length + raw bytes.
    pub fn qbytearray(&mut self, bytes: &[u8]) {
        self.u32(bytes.len() as u32);
        self.raw(bytes);
    }

    /// `QDateTime` under Qt_5_2: `i64` Julian day + `u32` milliseconds since
    /// midnight + `u8` timespec (1 = UTC — all archive timestamps are UTC,
    /// ADR-0009). `None` encodes Qt's null datetime: null `QDate`
    /// (`i64::MIN`), null `QTime` (`0xFFFFFFFF`), spec 0 (LocalTime).
    pub fn qdatetime(&mut self, t: Option<OffsetDateTime>) {
        match t {
            None => {
                self.i64(i64::MIN);
                self.u32(NULL_LENGTH);
                self.u8(0);
            }
            Some(t) => {
                let utc = t.to_offset(time::UtcOffset::UTC);
                self.i64(i64::from(utc.date().to_julian_day()));
                let time = utc.time();
                let ms = (u32::from(time.hour()) * 3600
                    + u32::from(time.minute()) * 60
                    + u32::from(time.second()))
                    * 1000
                    + time.millisecond() as u32;
                self.u32(ms);
                self.u8(1);
            }
        }
    }
}

/// Qt's `qCompress` framing: 4 bytes **big-endian** uncompressed length,
/// then a plain zlib stream. (The one big-endian island in these otherwise
/// little-endian blobs — it's Qt's own format, independent of the stream's
/// byte order.)
pub fn qcompress(data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    let mut encoder = ZlibEncoder::new(&mut out, Compression::new(9));
    encoder
        .write_all(data)
        .and_then(|()| encoder.finish().map(|_| ()))
        .expect("writing zlib to a Vec cannot fail");
    out
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use time::macros::datetime;

    fn bytes(f: impl FnOnce(&mut QtWriter)) -> Vec<u8> {
        let mut w = QtWriter::new();
        f(&mut w);
        w.into_bytes()
    }

    #[test]
    fn numeric_primitives_encode_little_endian() {
        assert_eq!(bytes(|w| w.u8(0xAB)), [0xAB]);
        assert_eq!(bytes(|w| w.i16(0x0102)), [0x02, 0x01]);
        assert_eq!(bytes(|w| w.u32(0x0102_0304)), [0x04, 0x03, 0x02, 0x01]);
        assert_eq!(bytes(|w| w.i32(-2)), [0xFE, 0xFF, 0xFF, 0xFF]);
        assert_eq!(
            bytes(|w| w.u64(0x0102_0304_0506_0708)),
            [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
        );
        assert_eq!(bytes(|w| w.f64(1.0)), 1.0f64.to_le_bytes());
    }

    #[test]
    fn qstring_null_and_empty_use_qt_sentinels() {
        assert_eq!(bytes(|w| w.qstring(None)), [0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(bytes(|w| w.qstring(Some(""))), [0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn qstring_encodes_utf16_little_endian_with_byte_length() {
        // ":/" — start of the icon resource paths seen in real blobs, where
        // ':' encodes as 3a 00 under the little-endian stream.
        assert_eq!(
            bytes(|w| w.qstring(Some(":/"))),
            [0x04, 0x00, 0x00, 0x00, 0x3A, 0x00, 0x2F, 0x00]
        );
        // Non-ASCII BMP char: 'å' = U+00E5.
        assert_eq!(
            bytes(|w| w.qstring(Some("Blåfjella"))),
            [
                0x12, 0x00, 0x00, 0x00, // 9 chars * 2 bytes
                0x42, 0x00, 0x6C, 0x00, 0xE5, 0x00, 0x66, 0x00, 0x6A, 0x00, 0x65, 0x00, 0x6C, 0x00,
                0x6C, 0x00, 0x61, 0x00,
            ]
        );
    }

    #[test]
    fn qbytearray_is_length_prefixed_raw_bytes() {
        assert_eq!(
            bytes(|w| w.qbytearray(&[0xDE, 0xAD])),
            [0x02, 0x00, 0x00, 0x00, 0xDE, 0xAD]
        );
    }

    #[test]
    fn qdatetime_encodes_julian_day_ms_and_utc_spec() {
        // Golden value verified against format_test.db during planning:
        // JD 2460925, 24044000 ms since midnight, spec 1 (UTC).
        let date = time::Date::from_julian_day(2460925).unwrap();
        let dt = date.with_hms_milli(6, 40, 44, 0).unwrap().assume_utc();
        let mut expected = 2460925i64.to_le_bytes().to_vec();
        expected.extend_from_slice(&24_044_000u32.to_le_bytes());
        expected.push(1);
        assert_eq!(bytes(|w| w.qdatetime(Some(dt))), expected);
    }

    #[test]
    fn qdatetime_null_matches_qt_null_encoding() {
        // Verified against null point times in Touren.db.
        let mut expected = i64::MIN.to_le_bytes().to_vec();
        expected.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        assert_eq!(bytes(|w| w.qdatetime(None)), expected);
    }

    #[test]
    fn qdatetime_normalizes_non_utc_offsets_to_utc() {
        let local = datetime!(2024-06-01 10:00:00 +02:00);
        let utc = datetime!(2024-06-01 08:00:00 UTC);
        assert_eq!(
            bytes(|w| w.qdatetime(Some(local))),
            bytes(|w| w.qdatetime(Some(utc)))
        );
    }

    #[test]
    fn qcompress_prefixes_big_endian_length_over_zlib_stream() {
        let data = b"QMapShack track payload, repeated: payload payload payload";
        let framed = qcompress(data);
        assert_eq!(&framed[..4], &(data.len() as u32).to_be_bytes());
        let mut inflated = Vec::new();
        flate2::read::ZlibDecoder::new(&framed[4..])
            .read_to_end(&mut inflated)
            .expect("valid zlib stream after the length prefix");
        assert_eq!(inflated, data);
    }
}
