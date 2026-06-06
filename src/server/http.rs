use axum::{
    extract::{Path, State},
    http::header,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use tower_http::services::ServeDir;

use crate::models::{TripDetail, TripSummary};
use crate::server::{error::AppError, import::handle_import, repo, state::AppState};

/// Build the application router. Shared by `main` and the integration tests so
/// both exercise the exact same routing (ADR-0012).
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(trip_list))
        .route("/import", get(import_form))
        .route("/api/import", post(handle_import))
        .route("/trips/:id", get(trip_detail))
        .route("/api/trips/:id/gpx", get(download_gpx))
        .route("/api/trips/:id/track.geojson", get(track_geojson))
        // Vendored, self-hosted map/chart assets and glue (ADR-0005/0006, US-10).
        // Resolved relative to the working directory (run from the project root),
        // matching the `./data` default in `main`; ADR-0014 defers deployment.
        .nest_service("/static", ServeDir::new("public"))
        .with_state(state)
}

/// GET `/` — the trip list, the archive's home (US-6).
async fn trip_list(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let trips = repo::list_trips(&state.pool).await?;
    Ok(Html(render_trip_list(&trips)))
}

/// GET `/import` — the import form (US-1: the owner uploads a GPX file).
async fn import_form() -> Html<&'static str> {
    Html(IMPORT_HTML)
}

/// GET `/trips/:id` — the trip detail page (the redirect target after import).
async fn trip_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Html<String>, AppError> {
    let trip = repo::get_trip(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Html(render_detail(&trip)))
}

/// GET `/api/trips/:id/gpx` — download the original uploaded GPX file
/// byte-for-byte, named after the trip (US-21).
async fn download_gpx(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let gpx = repo::get_original_gpx(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;

    let headers = [
        (header::CONTENT_TYPE, "application/gpx+xml".to_string()),
        (
            header::CONTENT_DISPOSITION,
            gpx_content_disposition(&gpx.name),
        ),
    ];
    Ok((headers, gpx.bytes).into_response())
}

/// GET `/api/trips/:id/track.geojson` — the track geometry as GeoJSON (US-7).
/// The client fetches this once to draw both the map polyline and the elevation
/// chart (geometry + elevation/distance arrays travel together; ADR-0005/0006).
async fn track_geojson(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let geojson = repo::get_track_geojson(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let headers = [(header::CONTENT_TYPE, "application/geo+json")];
    Ok((headers, geojson).into_response())
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

const IMPORT_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Trip Archive — Import</title>
</head>
<body>
  <h1>Import a Trip</h1>
  <form method="post" action="/api/import" enctype="multipart/form-data">
    <p>
      <label for="name">Trip name (optional)</label><br>
      <input type="text" id="name" name="name"
             placeholder="leave blank to use the GPX track name">
    </p>
    <p>
      <label for="activity_type">Activity (optional)</label><br>
      <select id="activity_type" name="activity_type">
        <option value="">— unspecified —</option>
        <option value="hiking">Hiking</option>
        <option value="cycling">Cycling</option>
        <option value="running">Running</option>
        <option value="skiing">Skiing</option>
      </select>
    </p>
    <p>
      <label for="gpx">GPX file</label><br>
      <input type="file" id="gpx" name="gpx" accept=".gpx,application/gpx+xml" required>
    </p>
    <button type="submit">Import</button>
  </form>
  <p><a href="/">← All trips</a></p>
</body>
</html>"#;

/// Render the trip detail page — relive a trip (US-7): the track on an OSM map
/// and an elevation profile, plus the summary stats and GPX download.
///
/// The map and chart are drawn client-side by `trip_detail.js`, which fetches
/// the track GeoJSON once and feeds both (ADR-0005/0006). The page only emits the
/// containers and the vendored, self-hosted assets (US-10); the server owns the
/// data URL and hands it to the script via the `data-track-url` attribute.
///
/// The photo gallery (the remaining part of US-7's acceptance) depends on the
/// photo stories (US-2…US-5) and lands with them.
fn render_detail(trip: &TripDetail) -> String {
    let distance_km = trip.distance_m / 1000.0;
    let ascent = trip.ascent_m.map(fmt_metres).unwrap_or_else(dash);
    let descent = trip.descent_m.map(fmt_metres).unwrap_or_else(dash);
    let duration = trip.duration_secs.map(fmt_duration).unwrap_or_else(dash);
    let start = trip.start_time.clone().unwrap_or_else(dash);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{name}</title>
  <link rel="stylesheet" href="/static/vendor/leaflet.css">
  <link rel="stylesheet" href="/static/vendor/uPlot.min.css">
  <style>
    #map {{ height: 24rem; }}
    #elevation {{ margin-top: 1rem; }}
  </style>
</head>
<body data-track-url="/api/trips/{id}/track.geojson">
  <h1>{name}</h1>
  <p><strong>Activity:</strong> {activity}</p>
  <p><strong>Start:</strong> {start}</p>
  <ul>
    <li>Distance: {distance:.2} km</li>
    <li>Ascent: {ascent}</li>
    <li>Descent: {descent}</li>
    <li>Duration: {duration}</li>
  </ul>

  <div id="map"></div>
  <div id="elevation"></div>

  <p><a href="/api/trips/{id}/gpx">Download original GPX</a></p>
  <p><a href="/">← All trips</a></p>

  <script src="/static/vendor/leaflet.js"></script>
  <script src="/static/vendor/uPlot.iife.min.js"></script>
  <script src="/static/js/trip_detail.js"></script>
</body>
</html>"#,
        id = trip.id,
        name = html_escape(&trip.name),
        activity = html_escape(&trip.activity_type),
        start = html_escape(&start),
        distance = distance_km,
        ascent = ascent,
        descent = descent,
        duration = duration,
    )
}

/// Render the trip list page (US-6). Shows each trip's name (linking to its
/// detail), date, distance, ascent, and duration; an empty state otherwise.
fn render_trip_list(trips: &[TripSummary]) -> String {
    let body = if trips.is_empty() {
        "<p>No trips yet. <a href=\"/import\">Import your first trip</a>.</p>".to_string()
    } else {
        let rows: String = trips.iter().map(render_trip_row).collect();
        format!(
            "<table>\n\
             <thead><tr><th>Trip</th><th>Date</th><th>Distance</th><th>Ascent</th><th>Duration</th></tr></thead>\n\
             <tbody>\n{rows}</tbody>\n\
             </table>"
        )
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Trip Archive</title>
</head>
<body>
  <h1>Trips</h1>
  <p><a href="/import">Import a trip</a></p>
  {body}
</body>
</html>"#
    )
}

/// One row of the trip list table.
fn render_trip_row(trip: &TripSummary) -> String {
    // start_time is RFC-3339 (e.g. "2024-06-01T08:00:00+00:00"); show the date part.
    let date = trip
        .start_time
        .as_deref()
        .and_then(|s| s.split('T').next())
        .unwrap_or("—");
    let distance_km = trip.distance_m / 1000.0;
    let ascent = trip.ascent_m.map(fmt_metres).unwrap_or_else(dash);
    let duration = trip.duration_secs.map(fmt_duration).unwrap_or_else(dash);

    format!(
        "<tr><td><a href=\"/trips/{id}\">{name}</a></td>\
         <td>{date}</td><td>{distance:.2} km</td><td>{ascent}</td><td>{duration}</td></tr>\n",
        id = trip.id,
        name = html_escape(&trip.name),
        date = html_escape(date),
        distance = distance_km,
        ascent = ascent,
        duration = duration,
    )
}

fn fmt_metres(m: f64) -> String {
    format!("{m:.0} m")
}

fn fmt_duration(secs: i64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    format!("{h:02}:{m:02}:{s:02}")
}

fn dash() -> String {
    "—".to_string()
}

/// Minimal HTML escaping for the small set of fields we interpolate.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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
