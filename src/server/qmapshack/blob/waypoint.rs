//! Builds the `items.data` blob for a QMapShack waypoint item (US-89): the
//! same one-event `history_t` as a track's, around a `QMWpt`-tagged chunk
//! (`CGisItemWpt::operator>>`, `serialization.cpp`; layout in
//! `docs/qmapshack-format.md`).

use time::OffsetDateTime;

use super::super::qtstream::{qcompress, QtWriter};
use super::super::VER_WPT;
use super::{build_history, lowercase_md5, write_wpt_base, WptBase};

/// 10-byte padded chunk magic for a waypoint (`serialization.cpp`).
pub const MAGIC_WPT: &[u8] = b"QMWpt     ";

/// Everything that varies per waypoint in its blob.
pub struct WaypointBlobInput<'a> {
    /// Goes into the chunk's `key.item`; must equal the row's `items.keyqms`.
    pub keyqms: &'a str,
    pub name: &'a str,
    pub desc: Option<&'a str>,
    pub lat: f64,
    pub lon: f64,
    pub ele: Option<f64>,
    /// The GPX `<sym>` as the file named it (US-95); `None` writes none.
    pub sym: Option<&'a str>,
}

/// Build the full `items.data` bytes plus the item hash. The hash covers the
/// chunk only, which holds no time, so the same waypoint always hashes the
/// same — what change detection compares (ADR-0022, 2026-10-11).
pub fn build_waypoint_item(
    input: &WaypointBlobInput<'_>,
    event_time: OffsetDateTime,
) -> (Vec<u8>, String) {
    let mut w = QtWriter::new();
    w.raw(MAGIC_WPT);
    w.u8(VER_WPT);
    w.qbytearray(&qcompress(&build_inner_fields(input)));
    let chunk = w.into_bytes();
    let hash = lowercase_md5(&chunk);
    (build_history(&chunk, &hash, event_time), hash)
}

/// The inner (pre-compression) waypoint field list, in `CGisItemWpt`'s exact
/// write order, with QMapShack's own defaults where the archive has nothing
/// (checked against every waypoint of a real QMapShack database).
fn build_inner_fields(input: &WaypointBlobInput<'_>) -> Vec<u8> {
    let mut w = QtWriter::new();
    w.qstring(Some(input.keyqms)); // key.item
    w.u32(0); // flags
    w.f64(super::NOFLOAT); // proximity: none
    write_wpt_base(
        &mut w,
        &WptBase {
            lat: input.lat,
            lon: input.lon,
            ele: input.ele.map(|e| e.round() as i32),
            time: None,
            name: Some(input.name),
            desc: input.desc,
            src: Some(super::WHO),
            sym: input.sym,
        },
    );
    w.u8(3); // geocache: VER_GC_T
    w.u8(0); // geocache.hasData: no
    w.u32(0); // images: empty QList
    w.i32(-320); // offsetBubble.x
    w.i32(-150); // offsetBubble.y
    w.u32(300); // widthBubble
    w.f64(0.0); // rating
    w.u32(0); // keywords: empty QSet
    w.into_bytes()
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::qmapshack::decode::decode_waypoint_item;
    use time::macros::datetime;

    fn input<'a>(sym: Option<&'a str>) -> WaypointBlobInput<'a> {
        WaypointBlobInput {
            keyqms: "trip-archive:waypoint:7",
            name: "Gjendebu",
            desc: Some("DNT-hytte"),
            lat: 61.48,
            lon: 8.62,
            ele: Some(995.6),
            sym,
        }
    }

    #[test]
    fn us89_built_blob_round_trips_through_the_strict_decoder() {
        let (data, hash) =
            build_waypoint_item(&input(Some("Lodge")), datetime!(2026-10-11 12:00 UTC));
        let decoded = decode_waypoint_item(&data).expect("decodes to exhaustion");
        assert_eq!(decoded.keyqms, "trip-archive:waypoint:7");
        assert_eq!(decoded.name.as_deref(), Some("Gjendebu"));
        assert_eq!(decoded.desc.as_deref(), Some("DNT-hytte"));
        assert_eq!((decoded.lat, decoded.lon), (61.48, 8.62));
        assert_eq!(decoded.ele, Some(996));
        assert_eq!(decoded.time, None);
        assert_eq!(decoded.sym.as_deref(), Some("Lodge"));
        assert_eq!(decoded.hash, hash);
        assert_eq!(decoded.chunk_md5, hash);
        assert_eq!(decoded.who.as_deref(), Some(super::super::WHO));
    }

    #[test]
    fn us89_a_waypoint_without_a_symbol_or_elevation_is_written_without() {
        let mut bare = input(None);
        bare.ele = None;
        bare.desc = None;
        let (data, _) = build_waypoint_item(&bare, datetime!(2026-10-11 12:00 UTC));
        let decoded = decode_waypoint_item(&data).expect("decodes");
        assert_eq!(decoded.sym, None);
        assert_eq!(decoded.ele, None);
        assert_eq!(decoded.desc, None);
    }

    #[test]
    fn us89_the_hash_depends_on_the_waypoint_not_on_when_it_was_written() {
        let (_, first) =
            build_waypoint_item(&input(Some("Lodge")), datetime!(2026-10-11 12:00 UTC));
        let (_, later) =
            build_waypoint_item(&input(Some("Lodge")), datetime!(2027-01-01 08:00 UTC));
        assert_eq!(first, later);
        let (_, moved) = build_waypoint_item(
            &WaypointBlobInput {
                lat: 61.49,
                ..input(Some("Lodge"))
            },
            datetime!(2026-10-11 12:00 UTC),
        );
        assert_ne!(first, moved);
        let (_, resymboled) =
            build_waypoint_item(&input(Some("Summit")), datetime!(2026-10-11 12:00 UTC));
        assert_ne!(first, resymboled);
    }
}
