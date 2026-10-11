//! Files a track is read as — its GPX for download and its GeoJSON for the
//! detail screens — whichever record keeps the track: a trip's, or a
//! collection track's (ADR-0028).

use axum::{
    http::header,
    response::{IntoResponse, Response},
};

use crate::server::{geojson, repo::GpxDownload};

/// A GPX file as a download named after what it holds — a trip, or anything
/// else that keeps its track's GPX.
pub(crate) fn gpx_download(gpx: GpxDownload) -> Response {
    let headers = [
        (header::CONTENT_TYPE, "application/gpx+xml".to_string()),
        (
            header::CONTENT_DISPOSITION,
            gpx_content_disposition(&gpx.name),
        ),
    ];
    (headers, gpx.bytes).into_response()
}

/// A stored track (ADR-0003) as the detail screens read it, whichever record
/// keeps it.
pub(crate) fn geojson_track(geojson: &str) -> Response {
    let headers = [(header::CONTENT_TYPE, "application/geo+json")];
    (headers, geojson::with_utc_offsets(geojson)).into_response()
}

/// Build an RFC 6266-compliant `Content-Disposition` for the GPX download:
/// a plain ASCII `filename` fallback plus a UTF-8 `filename*`, so non-ASCII trip
/// names (e.g. Norwegian "Tromsø") download with their real name in modern
/// browsers while older ones still get a usable name.
fn gpx_content_disposition(trip_name: &str) -> String {
    let filename = format!("{}.gpx", sanitize_filename(trip_name));
    let ascii_fallback: String = filename
        .chars()
        .map(|c| if c.is_ascii() { c } else { '_' })
        .collect();
    format!(
        "attachment; filename=\"{ascii_fallback}\"; filename*=UTF-8''{}",
        rfc5987_encode(&filename)
    )
}

/// Make a trip name safe as a filename: drop control characters and the bytes
/// that would break a header or imply a path. Keeps Unicode (handled by
/// `filename*`). Falls back to `trip` if nothing usable remains.
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | '"') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "trip".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Percent-encode a string for the `filename*` parameter value per RFC 5987:
/// keep the `attr-char` set literal, encode every other byte as `%XX`.
fn rfc5987_encode(s: &str) -> String {
    const ATTR_CHARS: &[u8] = b"!#$&+-.^_`|~";
    let mut out = String::with_capacity(s.len());
    for &byte in s.as_bytes() {
        if byte.is_ascii_alphanumeric() || ATTR_CHARS.contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// ── Tests (written first — ADR-0012) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // US-21: the download filename derived from the trip name.

    #[test]
    fn us21_disposition_uses_trip_name_for_ascii_names() {
        let cd = gpx_content_disposition("Oslo Hills Walk");
        assert!(
            cd.contains(r#"filename="Oslo Hills Walk.gpx""#),
            "ASCII fallback should be the trip name; got: {cd}"
        );
        assert!(
            cd.contains("filename*=UTF-8''Oslo%20Hills%20Walk.gpx"),
            "got: {cd}"
        );
    }

    #[test]
    fn us21_disposition_handles_non_ascii_names() {
        // "Tromsø" — ø is UTF-8 0xC3 0xB8.
        let cd = gpx_content_disposition("Tromsø");
        assert!(
            cd.contains(r#"filename="Troms_.gpx""#),
            "non-ASCII chars become _ in the ASCII fallback; got: {cd}"
        );
        assert!(
            cd.contains("filename*=UTF-8''Troms%C3%B8.gpx"),
            "filename* must be RFC-5987 percent-encoded UTF-8; got: {cd}"
        );
    }

    #[test]
    fn sanitize_filename_replaces_path_and_quote_characters() {
        assert_eq!(sanitize_filename("a/b\\c\"d"), "a_b_c_d");
    }

    #[test]
    fn sanitize_filename_falls_back_when_empty() {
        assert_eq!(sanitize_filename("   "), "trip");
    }
}
