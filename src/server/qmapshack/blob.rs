//! Builds the `items.data` blob for a QMapShack track item: a serialized
//! `IGisItem::history_t` with one history event whose payload is the
//! `QMTrk`-tagged, qCompress'd track chunk (`docs/qmapshack-format.md`,
//! field list ported from QMapShack's `serialization.cpp`).

use time::OffsetDateTime;

use super::qtstream::{qcompress, QtWriter};
use super::VER_TRK;

/// QMapShack's "not set" sentinels (`units/IUnit.h`).
pub const NOINT: i32 = 0x7FFF_FFFF;
pub const NOFLOAT: f64 = 1e12;

/// 10-byte padded chunk magic for a track (`serialization.cpp`).
pub const MAGIC_TRK: &[u8] = b"QMTrk     ";
pub const MAGIC_SIZE: usize = 10;

/// Cosmetic icon resource path recorded in the history event — the value
/// real QMapShack items carry; any resource path works.
const EVENT_ICON: &str = "://icons/48x48/Start.png";
const EVENT_COMMENT: &str = "Initial version.";
/// `history_event_t.who` / `items.last_user` for exporter-written items.
pub const WHO: &str = "trip-archive";

/// One track point as the exporter maps it out of the stored GeoJSON.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPointIn {
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<i32>,
    pub time: Option<OffsetDateTime>,
}

/// Everything that varies per trip in the track blob.
pub struct TrackBlobInput<'a> {
    /// Goes into the chunk's `key.item`; must equal the row's `items.keyqms`.
    pub keyqms: &'a str,
    pub name: &'a str,
    /// Best-effort summary; also used for `items.comment` by the caller.
    pub desc: Option<&'a str>,
    /// `trk.type` — the archive's stable activity wire name.
    pub trk_type: &'a str,
    /// A QMapShack color *name* (`IGisItem::init()`'s color map).
    pub color: &'a str,
    /// Trip tags, pre-sorted by the caller for deterministic output.
    pub keywords: &'a [String],
    pub points: &'a [TrackPointIn],
}

/// A malformed stored track GeoJSON blob (per-trip failure, not fatal).
#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("track geojson is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("track geojson has no {0} array")]
    MissingArray(&'static str),
    #[error("track geojson has {coords} coordinates but {timestamps} timestamps")]
    MismatchedArrays { coords: usize, timestamps: usize },
    #[error("track geojson coordinate {0} is not a [lon, lat, ...] number pair")]
    BadCoordinate(usize),
    #[error("track geojson timestamp {index} ({value:?}) is not RFC-3339")]
    BadTimestamp { index: usize, value: String },
    #[error("track has no points")]
    Empty,
}

/// Build the full `items.data` bytes plus the item hash (lowercase MD5 hex
/// of the chunk — what QMapShack stores in `items.hash`, see
/// `IGisItem::setupHistory`).
pub fn build_track_item(
    input: &TrackBlobInput<'_>,
    event_time: OffsetDateTime,
) -> (Vec<u8>, String) {
    let chunk = build_track_chunk(input);
    let hash = lowercase_md5(&chunk);

    // history_t with exactly one event (a freshly created item).
    let mut w = QtWriter::new();
    w.u8(1); // VER_HIST
    w.i32(0); // histIdxInitial
    w.i32(0); // histIdxCurrent
    w.u32(1); // one event
    w.u8(3); // VER_HIST_EVT
    w.qdatetime(Some(event_time));
    w.qstring(Some(EVENT_ICON));
    w.qstring(Some(EVENT_COMMENT));
    w.qbytearray(&chunk);
    w.qstring(Some(&hash));
    w.qstring(Some(WHO));
    (w.into_bytes(), hash)
}

/// The `QMTrk` chunk: magic + VER_TRK + qCompress'd inner field list.
fn build_track_chunk(input: &TrackBlobInput<'_>) -> Vec<u8> {
    let mut w = QtWriter::new();
    w.raw(MAGIC_TRK);
    w.u8(VER_TRK);
    w.qbytearray(&qcompress(&build_inner_fields(input)));
    w.into_bytes()
}

/// The inner (pre-compression) track field list, in `CGisItemTrk`'s exact
/// write order. Fields the archive has no source for get QMapShack's own
/// defaults (verified against reference blobs during planning).
fn build_inner_fields(input: &TrackBlobInput<'_>) -> Vec<u8> {
    let mut w = QtWriter::new();
    w.qstring(Some(input.keyqms)); // key.item
    w.u32(0); // flags
    w.qstring(Some(input.name)); // trk.name
    w.qstring(None); // trk.cmt
    w.qstring(input.desc); // trk.desc
    w.qstring(Some(WHO)); // trk.src
    w.u32(0); // trk.links: empty QList
    w.u64(0); // trk.number
    w.qstring(Some(input.trk_type)); // trk.type
    w.qstring(Some(input.color)); // trk.color
    w.f64(0.0); // rating
    w.u32(input.keywords.len() as u32); // keywords: QSet<QString>
    for keyword in input.keywords {
        w.qstring(Some(keyword));
    }

    write_climit(&mut w); // colorSourceLimit
    write_cvalue_double(&mut w, 1.0); // lineScale
    write_cvalue_bool(&mut w, true); // showArrows
    write_climit(&mut w); // limitsGraph1
    write_climit(&mut w); // limitsGraph2
    write_climit(&mut w); // limitsGraph3
    write_energy_defaults(&mut w); // energyCycling

    w.u32(1); // trk.segs: one segment holding every point
    w.u8(1); // VER_TRKSEG
    w.u32(input.points.len() as u32);
    for point in input.points {
        write_trkpt(&mut w, point);
    }
    w.into_bytes()
}

/// `CLimit` at its defaults: mode `eModeAuto` (2), no source, no user limits.
fn write_climit(w: &mut QtWriter) {
    w.u8(1); // VER_CLIMIT
    w.u8(2); // mode
    w.qstring(None); // source
    w.f64(NOFLOAT); // minUser
    w.f64(NOFLOAT); // maxUser
}

/// `CValue` with mode `eModeSys` (0) and a double user value (QVariant id 6).
fn write_cvalue_double(w: &mut QtWriter, value: f64) {
    w.u8(1); // VER_CVALUE
    w.u8(0); // mode
    w.u32(6); // QVariant type: Double
    w.u8(0); // not null
    w.f64(value);
}

/// `CValue` with mode `eModeSys` (0) and a bool user value (QVariant id 1).
fn write_cvalue_bool(w: &mut QtWriter, value: bool) {
    w.u8(1); // VER_CVALUE
    w.u8(0); // mode
    w.u32(1); // QVariant type: Bool
    w.u8(0); // not null
    w.u8(u8::from(value));
}

/// `energy_set_t` at its defaults (`CEnergyCycling.h`) — the archive has no
/// cycling-energy model; QMapShack shows these as "not computed".
fn write_energy_defaults(w: &mut QtWriter) {
    w.u8(1); // VER_ENERGYCYCLE
    w.f64(75.0); // driverWeight
    w.f64(15.0); // bikeWeight
    w.f64(1.2); // airDensity
    w.i32(5); // windSpeedIndex
    w.f64(0.0); // windSpeed
    w.i32(2); // windPositionIndex
    w.f64(0.65); // frontalArea
    w.f64(1.0); // windDragCoeff
    w.i32(3); // groundIndex
    w.f64(0.005); // rollingCoeff
    w.f64(75.0); // pedalCadence
    w.f64(NOFLOAT); // energyKcal
}

/// One `trkpt_t` (version 3): flags + `wpt_t` base + empty extensions +
/// activity "none".
fn write_trkpt(w: &mut QtWriter, point: &TrackPointIn) {
    w.u8(3); // VER_TRKPT
    w.u32(0); // flags
    w.u8(1); // VER_WPT_T
    w.f64(point.lat);
    w.f64(point.lon);
    w.i32(point.ele.unwrap_or(NOINT));
    w.qdatetime(point.time);
    w.i32(NOINT); // magvar
    w.i32(NOINT); // geoidheight
    for _ in 0..4 {
        w.qstring(None); // name, cmt, desc, src
    }
    w.u32(0); // links
    for _ in 0..3 {
        w.qstring(None); // sym, type, fix
    }
    for _ in 0..6 {
        w.i32(NOINT); // sat, hdop, vdop, pdop, ageofdgpsdata, dgpsid
    }
    w.u32(0); // extensions: empty QHash
    w.i16(0); // activity: none
}

fn lowercase_md5(bytes: &[u8]) -> String {
    use md5::{Digest, Md5};
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Parse the archive's stored track GeoJSON (ADR-0003 Feature shape built by
/// `geojson::build_track_geojson`) into exportable points: `[lon, lat, ele]`
/// coordinates plus the parallel `properties.timestamps` array (`""` = no
/// time). Strict — a malformed blob fails the trip, not the run.
pub fn points_from_geojson(geojson: &str) -> Result<Vec<TrackPointIn>, BlobError> {
    let value: serde_json::Value = serde_json::from_str(geojson)?;
    let coordinates = value["geometry"]["coordinates"]
        .as_array()
        .ok_or(BlobError::MissingArray("geometry.coordinates"))?;
    let timestamps = value["properties"]["timestamps"]
        .as_array()
        .ok_or(BlobError::MissingArray("properties.timestamps"))?;
    if coordinates.len() != timestamps.len() {
        return Err(BlobError::MismatchedArrays {
            coords: coordinates.len(),
            timestamps: timestamps.len(),
        });
    }
    if coordinates.is_empty() {
        return Err(BlobError::Empty);
    }

    coordinates
        .iter()
        .zip(timestamps)
        .enumerate()
        .map(|(index, (coord, ts))| {
            let lon = coord.get(0).and_then(|v| v.as_f64());
            let lat = coord.get(1).and_then(|v| v.as_f64());
            let (Some(lon), Some(lat)) = (lon, lat) else {
                return Err(BlobError::BadCoordinate(index));
            };
            // build_track_geojson always writes an elevation (0.0 when the
            // GPX had none) — that lossiness is upstream of the exporter.
            let ele = coord
                .get(2)
                .and_then(|v| v.as_f64())
                .map(|e| e.round() as i32);
            let time = match ts.as_str() {
                None | Some("") => None,
                Some(raw) => Some(
                    OffsetDateTime::parse(raw, &time::format_description::well_known::Rfc3339)
                        .map_err(|_| BlobError::BadTimestamp {
                            index,
                            value: raw.to_string(),
                        })?,
                ),
            };
            Ok(TrackPointIn {
                lat,
                lon,
                ele,
                time,
            })
        })
        .collect()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::qmapshack::decode::decode_track_item;
    use time::macros::datetime;

    fn sample_input<'a>(keywords: &'a [String], points: &'a [TrackPointIn]) -> TrackBlobInput<'a> {
        TrackBlobInput {
            keyqms: "trip-archive:trip:42",
            name: "Blåfjella på langs",
            desc: Some("2024-06-01 · Hiking\n12.3 km"),
            trk_type: "hiking",
            color: "DarkRed",
            keywords,
            points,
        }
    }

    fn sample_points() -> Vec<TrackPointIn> {
        vec![
            TrackPointIn {
                lat: 59.91,
                lon: 10.75,
                ele: Some(100),
                time: Some(datetime!(2024-06-01 08:00:00 UTC)),
            },
            TrackPointIn {
                lat: 59.92,
                lon: 10.76,
                ele: None,
                time: None,
            },
            TrackPointIn {
                lat: 59.93,
                lon: 10.77,
                ele: Some(-12),
                time: Some(datetime!(2024-06-01 08:20:00.500 UTC)),
            },
        ]
    }

    #[test]
    fn built_blob_round_trips_through_the_strict_decoder() {
        let keywords = vec!["fjell".to_string(), "telt".to_string()];
        let points = sample_points();
        let (data, hash) = build_track_item(
            &sample_input(&keywords, &points),
            datetime!(2026-07-23 12:00:00 UTC),
        );

        let decoded = decode_track_item(&data).expect("strict decode of our own blob");
        assert_eq!(decoded.keyqms, "trip-archive:trip:42");
        assert_eq!(decoded.name.as_deref(), Some("Blåfjella på langs"));
        assert_eq!(
            decoded.desc.as_deref(),
            Some("2024-06-01 · Hiking\n12.3 km")
        );
        assert_eq!(decoded.trk_type.as_deref(), Some("hiking"));
        assert_eq!(decoded.color.as_deref(), Some("DarkRed"));
        assert_eq!(decoded.keywords, keywords);
        assert_eq!(decoded.who.as_deref(), Some(WHO));
        assert_eq!(decoded.hash, hash);
        assert_eq!(decoded.chunk_md5, hash, "hash is md5 of the chunk");

        let expected: Vec<_> = points
            .iter()
            .map(|p| crate::server::qmapshack::decode::DecodedPoint {
                lat: p.lat,
                lon: p.lon,
                ele: p.ele,
                time: p.time,
            })
            .collect();
        assert_eq!(decoded.points, expected);
    }

    #[test]
    fn hash_is_deterministic_for_identical_input_and_event_time() {
        let points = sample_points();
        let input = sample_input(&[], &points);
        let t = datetime!(2026-07-23 12:00:00 UTC);
        let (data_a, hash_a) = build_track_item(&input, t);
        let (data_b, hash_b) = build_track_item(&input, t);
        assert_eq!(hash_a, hash_b);
        assert_eq!(data_a, data_b);
    }

    #[test]
    fn points_from_geojson_round_trips_the_stored_blob_format() {
        use crate::server::gpx::TrackPoint;
        let source = vec![
            TrackPoint {
                lat: 59.91,
                lon: 10.75,
                ele: Some(100.4),
                time: Some(datetime!(2024-06-01 08:00:00 UTC)),
            },
            TrackPoint {
                lat: 59.92,
                lon: 10.76,
                ele: None,
                time: None,
            },
        ];
        let geojson = crate::server::geojson::build_track_geojson(&source);

        let points = points_from_geojson(&geojson).expect("valid stored blob parses");
        assert_eq!(points.len(), 2);
        assert!((points[0].lat - 59.91).abs() < 1e-9);
        assert!((points[0].lon - 10.75).abs() < 1e-9);
        assert_eq!(points[0].ele, Some(100), "elevation rounds to whole metres");
        assert_eq!(points[0].time, Some(datetime!(2024-06-01 08:00:00 UTC)));
        // build_track_geojson stores missing elevation as 0.0 — lossy upstream.
        assert_eq!(points[1].ele, Some(0));
        assert_eq!(points[1].time, None, "empty timestamp means no time");
    }

    #[test]
    fn points_from_geojson_rejects_garbage_and_empty_tracks() {
        assert!(matches!(
            points_from_geojson("garbage"),
            Err(BlobError::Json(_))
        ));
        assert!(matches!(
            points_from_geojson(r#"{"type":"Feature"}"#),
            Err(BlobError::MissingArray(_))
        ));
        let empty = r#"{"geometry":{"coordinates":[]},"properties":{"timestamps":[]}}"#;
        assert!(matches!(points_from_geojson(empty), Err(BlobError::Empty)));
        let mismatched =
            r#"{"geometry":{"coordinates":[[1.0,2.0,0.0]]},"properties":{"timestamps":[]}}"#;
        assert!(matches!(
            points_from_geojson(mismatched),
            Err(BlobError::MismatchedArrays { .. })
        ));
        let bad_time = r#"{"geometry":{"coordinates":[[1.0,2.0,0.0]]},"properties":{"timestamps":["yesterday"]}}"#;
        assert!(matches!(
            points_from_geojson(bad_time),
            Err(BlobError::BadTimestamp { .. })
        ));
    }
}
