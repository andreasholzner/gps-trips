//! Hand-built minimal TIFF/EXIF byte fixtures, real enough for `kamadak-exif`
//! to parse for real (ADR-0012, ADR-0017) — no image tooling
//! (PIL/piexif/exiftool) is available in this environment, and none is
//! needed. `kamadak-exif`'s container sniffing recognizes a bare TIFF byte
//! stream directly (a TIFF file *is* the EXIF IFD structure — no JPEG
//! SOI/APP1 wrapping needed), so these are plain, minimal TIFF files: an
//! 8-byte header, an IFD0 with a single GPSInfo-IFD-pointer entry, and a GPS
//! IFD with the four tags this module reads.
//!
//! `pub`, gated by `test-support` as well as `test`, so both `location`'s own
//! unit tests *and* `tests/us3_photo_map_placement.rs`'s integration tests
//! call the same byte-builder — see the `[dev-dependencies]` entry in
//! `Cargo.toml` that enables `test-support` for `tests/`. A single source of
//! truth for the TIFF layout, instead of two hand-maintained copies.

const TIFF_HEADER_LEN: u32 = 8;

fn u16_le(n: u16) -> [u8; 2] {
    n.to_le_bytes()
}
fn u32_le(n: u32) -> [u8; 4] {
    n.to_le_bytes()
}

/// One EXIF IFD entry: tag, type, count, and a 4-byte value/offset field.
fn ifd_entry(tag: u16, kind: u16, count: u32, value_or_offset: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&u16_le(tag));
    out.extend_from_slice(&u16_le(kind));
    out.extend_from_slice(&u32_le(count));
    out.extend_from_slice(&value_or_offset);
    out
}

/// An ASCII EXIF value up to 4 bytes (incl. the trailing NUL), inline in
/// the entry's value field — exactly how `GPSLatitudeRef`/`GPSLongitudeRef`
/// ("N\0", "S\0", ...) are stored.
fn inline_ascii(byte: u8) -> [u8; 4] {
    [byte, 0, 0, 0]
}

/// Encode `decimal` degrees as an EXIF (degrees, minutes, seconds)
/// rational triple: 8 bytes each (numerator/denominator, both u32 LE).
/// Seconds keep 4 decimal digits of precision (denominator 10000).
fn dms_rationals(decimal: f64) -> Vec<u8> {
    let abs = decimal.abs();
    let deg = abs.floor();
    let min_full = (abs - deg) * 60.0;
    let min = min_full.floor();
    let sec = (min_full - min) * 60.0;

    let mut out = Vec::with_capacity(24);
    out.extend_from_slice(&u32_le(deg as u32));
    out.extend_from_slice(&u32_le(1));
    out.extend_from_slice(&u32_le(min as u32));
    out.extend_from_slice(&u32_le(1));
    out.extend_from_slice(&u32_le((sec * 10_000.0).round() as u32));
    out.extend_from_slice(&u32_le(10_000));
    out
}

/// An ASCII EXIF value that doesn't fit inline (more than 4 bytes incl.
/// the trailing NUL) — `DateTimeOriginal`/`OffsetTimeOriginal` values are
/// always stored out-of-line, unlike the single-byte GPS refs above.
fn ascii_bytes(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

const TYPE_ASCII: u16 = 2;
const TYPE_SHORT: u16 = 3;
const TYPE_LONG: u16 = 4;
const TYPE_RATIONAL: u16 = 5;
const TAG_ORIENTATION: u16 = 0x0112;
const TAG_GPS_INFO_IFD_POINTER: u16 = 0x8825;
const TAG_GPS_LATITUDE_REF: u16 = 1;
const TAG_GPS_LATITUDE: u16 = 2;
const TAG_GPS_LONGITUDE_REF: u16 = 3;
const TAG_GPS_LONGITUDE: u16 = 4;
const TAG_EXIF_IFD_POINTER: u16 = 0x8769;
const TAG_DATE_TIME_ORIGINAL: u16 = 0x9003;
const TAG_OFFSET_TIME_ORIGINAL: u16 = 0x9011;

/// Assemble a minimal little-endian TIFF/EXIF byte stream from IFD0
/// pointer entries: each `(tag, sub_ifd_bytes)` pair becomes one IFD0
/// entry pointing at `sub_ifd_bytes`, laid out sequentially right after
/// IFD0 itself, in the order given. Every fixture below is IFD0 plus one
/// or two such sub-IFDs (GPS, Exif/capture-time, or both) — this is the
/// one place that outer header+IFD0+concatenation shape is assembled,
/// instead of each fixture re-deriving it.
///
/// Callers still compute each sub-IFD's own *internal* offsets themselves
/// (via `gps_ifd_at`/`exif_ifd_at`'s explicit `start` parameter) — that
/// arithmetic is inherently sequential (a sub-IFD's contents depend on
/// knowing where it starts), so only the outer assembly is shared here.
fn assemble_tiff(sub_ifds: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let ifd0_len = 2 + 12 * sub_ifds.len() as u32 + 4;
    let mut offset = TIFF_HEADER_LEN + ifd0_len;
    let offsets: Vec<u32> = sub_ifds
        .iter()
        .map(|(_, bytes)| {
            let this = offset;
            offset += bytes.len() as u32;
            this
        })
        .collect();

    let mut out = Vec::new();
    out.extend_from_slice(b"II"); // little-endian byte order
    out.extend_from_slice(&u16_le(42)); // TIFF magic number
    out.extend_from_slice(&u32_le(TIFF_HEADER_LEN)); // IFD0 starts right after the header

    out.extend_from_slice(&u16_le(sub_ifds.len() as u16));
    for ((tag, _), sub_ifd_offset) in sub_ifds.iter().zip(&offsets) {
        out.extend_from_slice(&ifd_entry(*tag, TYPE_LONG, 1, u32_le(*sub_ifd_offset)));
    }
    out.extend_from_slice(&u32_le(0)); // no next IFD after IFD0

    for (_, bytes) in sub_ifds {
        out.extend_from_slice(bytes);
    }
    out
}

/// Build a minimal little-endian TIFF/EXIF byte stream. `gps_ifd` is the
/// already-assembled GPS IFD (or `None` to omit it entirely, producing a
/// TIFF with no GPS data at all).
pub fn build_tiff(gps_ifd: Option<Vec<u8>>) -> Vec<u8> {
    match gps_ifd {
        None => assemble_tiff(&[]),
        Some(gps_ifd) => assemble_tiff(&[(TAG_GPS_INFO_IFD_POINTER, gps_ifd)]),
    }
}

/// A GPS IFD with all four tags this module reads, rationals stored
/// after the IFD itself (they don't fit inline in a 4-byte value field).
/// Assumes IFD0 has exactly one entry (the GPS pointer), i.e. this GPS
/// IFD is the only sub-IFD in the fixture.
pub fn gps_ifd(lat: f64, lat_ref: u8, lon: f64, lon_ref: u8) -> Vec<u8> {
    // 8 (header) + 2+12+4 (IFD0 with 1 entry) = offset where this GPS IFD begins.
    let gps_ifd_start = TIFF_HEADER_LEN + 2 + 12 + 4;
    gps_ifd_at(gps_ifd_start, lat, lat_ref, lon, lon_ref)
}

/// As `gps_ifd`, but for a GPS IFD that doesn't necessarily start right
/// after a 1-entry IFD0 — used when IFD0 also has an Exif-pointer entry
/// (US-4's combined GPS + capture-time fixture).
fn gps_ifd_at(gps_ifd_start: u32, lat: f64, lat_ref: u8, lon: f64, lon_ref: u8) -> Vec<u8> {
    let ifd_body_len = 2 + 4 * 12 + 4; // count + 4 entries + next-IFD offset
    let lat_rationals_offset = gps_ifd_start + ifd_body_len;
    let lon_rationals_offset = lat_rationals_offset + 24;

    let mut ifd = Vec::new();
    ifd.extend_from_slice(&u16_le(4)); // 4 entries
    ifd.extend_from_slice(&ifd_entry(
        TAG_GPS_LATITUDE_REF,
        TYPE_ASCII,
        2,
        inline_ascii(lat_ref),
    ));
    ifd.extend_from_slice(&ifd_entry(
        TAG_GPS_LATITUDE,
        TYPE_RATIONAL,
        3,
        u32_le(lat_rationals_offset),
    ));
    ifd.extend_from_slice(&ifd_entry(
        TAG_GPS_LONGITUDE_REF,
        TYPE_ASCII,
        2,
        inline_ascii(lon_ref),
    ));
    ifd.extend_from_slice(&ifd_entry(
        TAG_GPS_LONGITUDE,
        TYPE_RATIONAL,
        3,
        u32_le(lon_rationals_offset),
    ));
    ifd.extend_from_slice(&u32_le(0)); // no next IFD after the GPS IFD

    ifd.extend_from_slice(&dms_rationals(lat));
    ifd.extend_from_slice(&dms_rationals(lon));
    ifd
}

/// A GPS IFD whose latitude reference byte is neither `N` nor `S`.
pub fn gps_ifd_with_malformed_ref(lat: f64, lon: f64, lon_ref: u8) -> Vec<u8> {
    gps_ifd(lat, b'X', lon, lon_ref)
}

/// A minimal geotagged TIFF/EXIF byte stream for `lat`/`lon` (signed
/// decimal degrees).
pub fn geotagged_bytes(lat: f64, lon: f64) -> Vec<u8> {
    let lat_ref = if lat < 0.0 { b'S' } else { b'N' };
    let lon_ref = if lon < 0.0 { b'W' } else { b'E' };
    build_tiff(Some(gps_ifd(lat, lat_ref, lon, lon_ref)))
}

/// An Exif SubIFD carrying `DateTimeOriginal` (EXIF's fixed
/// "YYYY:MM:DD HH:MM:SS" format) and, optionally, `OffsetTimeOriginal`
/// ("+HH:MM"/"-HH:MM") — both values exceed 4 bytes, so (like the GPS
/// rationals) they're stored after the IFD itself, never inline.
fn exif_ifd_at(start: u32, datetime_original: &str, offset_original: Option<&str>) -> Vec<u8> {
    let entry_count = 1 + offset_original.is_some() as u16;
    let ifd_body_len = 2 + 12 * entry_count as u32 + 4;

    let dt_bytes = ascii_bytes(datetime_original);
    let dt_offset = start + ifd_body_len;
    let offset_bytes = offset_original.map(ascii_bytes);
    let offset_field_offset = dt_offset + dt_bytes.len() as u32;

    let mut ifd = Vec::new();
    ifd.extend_from_slice(&u16_le(entry_count));
    ifd.extend_from_slice(&ifd_entry(
        TAG_DATE_TIME_ORIGINAL,
        TYPE_ASCII,
        dt_bytes.len() as u32,
        u32_le(dt_offset),
    ));
    if let Some(ref ob) = offset_bytes {
        ifd.extend_from_slice(&ifd_entry(
            TAG_OFFSET_TIME_ORIGINAL,
            TYPE_ASCII,
            ob.len() as u32,
            u32_le(offset_field_offset),
        ));
    }
    ifd.extend_from_slice(&u32_le(0)); // no next IFD after this one

    ifd.extend_from_slice(&dt_bytes);
    if let Some(ob) = offset_bytes {
        ifd.extend_from_slice(&ob);
    }
    ifd
}

/// A minimal TIFF/EXIF byte stream carrying only a capture time (no GPS)
/// — IFD0 has a single entry, the Exif-SubIFD pointer.
pub fn capture_time_bytes(datetime_original: &str, offset_original: Option<&str>) -> Vec<u8> {
    const IFD0_LEN: u32 = 2 + 12 + 4; // count + 1 entry + next-IFD
    let exif_start = TIFF_HEADER_LEN + IFD0_LEN;
    let exif_ifd = exif_ifd_at(exif_start, datetime_original, offset_original);
    assemble_tiff(&[(TAG_EXIF_IFD_POINTER, exif_ifd)])
}

/// A geotagged TIFF/EXIF byte stream that also carries a capture time
/// (US-4: proves EXIF GPS still wins over interpolation even when a
/// valid capture time is also present). IFD0 has two entries: the GPS
/// pointer, then the Exif pointer.
pub fn geotagged_bytes_with_capture_time(lat: f64, lon: f64, datetime_original: &str) -> Vec<u8> {
    let lat_ref = if lat < 0.0 { b'S' } else { b'N' };
    let lon_ref = if lon < 0.0 { b'W' } else { b'E' };

    const IFD0_LEN: u32 = 2 + 12 * 2 + 4; // count + 2 entries + next-IFD
    let gps_start = TIFF_HEADER_LEN + IFD0_LEN;
    let gps = gps_ifd_at(gps_start, lat, lat_ref, lon, lon_ref);
    let exif_start = gps_start + gps.len() as u32;
    let exif = exif_ifd_at(exif_start, datetime_original, None);

    assemble_tiff(&[
        (TAG_GPS_INFO_IFD_POINTER, gps),
        (TAG_EXIF_IFD_POINTER, exif),
    ])
}

/// A minimal TIFF/EXIF byte stream carrying only an `Orientation` tag (IFD0,
/// no GPS/Exif sub-IFD) — same isolation style as `capture_time_bytes`, since
/// `read_orientation` only needs IFD0 to have the tag, nothing else.
pub fn orientation_bytes(value: u16) -> Vec<u8> {
    let mut ifd0 = Vec::new();
    ifd0.extend_from_slice(&u16_le(1)); // 1 entry
    ifd0.extend_from_slice(&ifd_entry(
        TAG_ORIENTATION,
        TYPE_SHORT,
        1,
        u32_le(value as u32),
    ));
    ifd0.extend_from_slice(&u32_le(0)); // no next IFD

    let mut out = Vec::new();
    out.extend_from_slice(b"II");
    out.extend_from_slice(&u16_le(42));
    out.extend_from_slice(&u32_le(TIFF_HEADER_LEN));
    out.extend_from_slice(&ifd0);
    out
}

/// A TIFF/EXIF byte stream whose `Orientation` tag is a non-numeric (ASCII)
/// value — `read_orientation` must treat this as absent rather than
/// misreading garbage as a value.
pub fn malformed_orientation_bytes() -> Vec<u8> {
    let mut ifd0 = Vec::new();
    ifd0.extend_from_slice(&u16_le(1)); // 1 entry
    ifd0.extend_from_slice(&ifd_entry(
        TAG_ORIENTATION,
        TYPE_ASCII,
        2,
        inline_ascii(b'X'),
    ));
    ifd0.extend_from_slice(&u32_le(0)); // no next IFD

    let mut out = Vec::new();
    out.extend_from_slice(b"II");
    out.extend_from_slice(&u16_le(42));
    out.extend_from_slice(&u32_le(TIFF_HEADER_LEN));
    out.extend_from_slice(&ifd0);
    out
}
